#![allow(dead_code)]
use anyhow::{Context, Result};
use std::path::Path;
use std::process::{Command, Stdio};
use super::{Translator, TranslationResult};

/// Deterministic 1:1 MT backend (no LLM inference), powered by CTranslate2 + SentencePiece.
///
/// Runtime resolution order:
/// - Bundled runtime under `resources/mt-runtime` (portable/release first choice)
/// - Explicit Python from `LOCALTRANS_MT_PYTHON`
/// - Conda fallback (`LOCALTRANS_MT_CONDA_ENV` or `localtrans`)
pub struct NllbTranslator {
    model_path: Option<String>,
    python_exe: String,
    conda_exe: Option<String>,
    conda_env_name: String,
}

impl NllbTranslator {
    pub fn new() -> Self {
        Self {
            model_path: None,
            python_exe: resolve_python_exe(),
            conda_exe: resolve_conda_exe(),
            conda_env_name: std::env::var("LOCALTRANS_MT_CONDA_ENV")
                .ok()
                .filter(|v| !v.trim().is_empty())
                .unwrap_or_else(|| "localtrans".to_string()),
        }
    }

    fn run_argos_translate(&self, text: &str, source_lang: &str, target_lang: &str) -> Result<String> {
        let inline_script = "import os,sys,json,pathlib,ctranslate2,sentencepiece as spm;text,src,dst=sys.argv[1],sys.argv[2],sys.argv[3];root=os.environ.get('ARGOS_PACKAGES_DIR') or str(pathlib.Path.home()/'.local/share/argos-translate/packages');rp=pathlib.Path(root);pkg=None\nfor d in rp.iterdir() if rp.exists() else []:\n m=d/'metadata.json'\n if m.exists():\n  md=json.loads(m.read_text(encoding='utf-8'))\n  if md.get('from_code')==src and md.get('to_code')==dst:\n   pkg=d;break\nif pkg is None: raise RuntimeError(f'No Argos package for {src}->{dst} under {root}');sp=spm.SentencePieceProcessor(model_file=str(pkg/'sentencepiece.model'));tr=ctranslate2.Translator(str(pkg/'model'),device='cpu');pieces=sp.encode(text,out_type=str);res=tr.translate_batch([pieces],beam_size=1,max_decoding_length=256,no_repeat_ngram_size=3);out=''.join(res[0].hypotheses[0]).replace('▁',' ').strip();print(out,end='')";
        let bundled_script = resolve_bundled_mt_script();
        let run_direct = || -> Result<std::process::Output> {
            let mut cmd = Command::new(&self.python_exe);
            if let Some(argos_dir) = resolve_bundled_argos_packages() {
                cmd.env("ARGOS_PACKAGES_DIR", argos_dir);
            }
            if let Some(script_path) = bundled_script.as_ref() {
                cmd.arg(script_path);
            } else {
                cmd.arg("-c").arg(inline_script);
            }
            cmd
                .arg(text)
                .arg(source_lang)
                .arg(target_lang)
                .env("PYTHONIOENCODING", "utf-8")
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .output()
                .with_context(|| {
                    format!(
                        "failed to start MT python process (python={})",
                        self.python_exe
                    )
                })
        };

        let run_conda = || -> Result<std::process::Output> {
            let conda = self
                .conda_exe
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("conda executable not found"))?;
            let mut cmd = Command::new(conda);
            if let Some(argos_dir) = resolve_bundled_argos_packages() {
                cmd.env("ARGOS_PACKAGES_DIR", argos_dir);
            }
            cmd
                .arg("run")
                .arg("--no-capture-output")
                .arg("-n")
                .arg(&self.conda_env_name)
                .arg("python");
            if let Some(script_path) = bundled_script.as_ref() {
                cmd.arg(script_path);
            } else {
                cmd.arg("-c").arg(inline_script);
            }
            cmd
                .arg(text)
                .arg(source_lang)
                .arg(target_lang)
                .env("PYTHONIOENCODING", "utf-8")
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .output()
                .with_context(|| {
                    format!(
                        "failed to start MT via conda (conda={}, env={})",
                        conda, self.conda_env_name
                    )
                })
        };

        let output = match run_direct() {
            Ok(out) if out.status.success() => out,
            Ok(_) | Err(_) => run_conda()?,
        };

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            anyhow::bail!(
                "Argos MT failed for {}->{}: {}",
                source_lang,
                target_lang,
                if stderr.is_empty() { "unknown error".to_string() } else { stderr }
            );
        }

        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }
}

impl Translator for NllbTranslator {
    fn init(model_path: &Path) -> Result<Self> {
        Ok(Self {
            model_path: Some(model_path.to_string_lossy().to_string()),
            python_exe: resolve_python_exe(),
            conda_exe: resolve_conda_exe(),
            conda_env_name: std::env::var("LOCALTRANS_MT_CONDA_ENV")
                .ok()
                .filter(|v| !v.trim().is_empty())
                .unwrap_or_else(|| "localtrans".to_string()),
        })
    }

    fn translate(&mut self, text: &str, source_lang: &str, target_lang: &str) -> Result<TranslationResult> {
        if source_lang.eq_ignore_ascii_case(target_lang) {
            return Ok(TranslationResult {
                text: text.to_string(),
                source_lang: source_lang.to_string(),
                target_lang: target_lang.to_string(),
                confidence: 1.0,
            });
        }
        let translated = self.run_argos_translate(text, source_lang, target_lang)?;
        Ok(TranslationResult {
            text: translated,
            source_lang: source_lang.to_string(),
            target_lang: target_lang.to_string(),
            confidence: 0.92,
        })
    }

    fn get_supported_languages(&self) -> Vec<(String, String)> {
        vec![
            ("en".to_string(), "English".to_string()),
            ("zh".to_string(), "Chinese".to_string()),
            ("ja".to_string(), "Japanese".to_string()),
            ("ko".to_string(), "Korean".to_string()),
            ("fr".to_string(), "French".to_string()),
            ("de".to_string(), "German".to_string()),
            ("es".to_string(), "Spanish".to_string()),
            ("ru".to_string(), "Russian".to_string()),
        ]
    }
}

impl Default for NllbTranslator {
    fn default() -> Self {
        Self::new()
    }
}

fn resolve_python_exe() -> String {
    if let Some(p) = resolve_bundled_python() {
        return p;
    }
    if let Ok(p) = std::env::var("LOCALTRANS_MT_PYTHON") {
        let v = p.trim();
        if !v.is_empty() {
            return v.to_string();
        }
    }
    if let Ok(prefix) = std::env::var("CONDA_PREFIX") {
        let p = std::path::Path::new(&prefix).join("python.exe");
        if p.exists() {
            return p.display().to_string();
        }
    }
    if let Some(home) = dirs::home_dir() {
        let candidate = home
            .join(".conda")
            .join("envs")
            .join("localtrans")
            .join("python.exe");
        if candidate.exists() {
            return candidate.display().to_string();
        }
    }
    "python".to_string()
}

fn resolve_bundled_python() -> Option<String> {
    let exe = std::env::current_exe().ok()?;
    let exe_dir = exe.parent()?;

    let candidates = [
        exe_dir.join("resources").join("mt-runtime").join("python").join("python.exe"),
        exe_dir.join("mt-runtime").join("python").join("python.exe"),
        exe_dir.join("python").join("python.exe"),
    ];

    for p in candidates {
        if p.exists() {
            return Some(p.display().to_string());
        }
    }
    None
}

fn resolve_bundled_argos_packages() -> Option<String> {
    let exe = std::env::current_exe().ok()?;
    let exe_dir = exe.parent()?;
    let candidates = [
        exe_dir.join("resources").join("mt-runtime").join("argos-packages"),
        exe_dir.join("mt-runtime").join("argos-packages"),
    ];
    for p in candidates {
        if p.exists() {
            return Some(p.display().to_string());
        }
    }
    None
}

fn resolve_bundled_mt_script() -> Option<String> {
    let exe = std::env::current_exe().ok()?;
    let exe_dir = exe.parent()?;
    let candidates = [
        exe_dir.join("resources").join("mt-runtime").join("mt_translate.py"),
        exe_dir.join("mt-runtime").join("mt_translate.py"),
    ];
    for p in candidates {
        if p.exists() {
            return Some(p.display().to_string());
        }
    }
    None
}

fn resolve_conda_exe() -> Option<String> {
    if let Ok(v) = std::env::var("CONDA_EXE") {
        let v = v.trim();
        if !v.is_empty() && std::path::Path::new(v).exists() {
            return Some(v.to_string());
        }
    }
    let candidates = [
        r"D:\miniconda3\Scripts\conda.exe",
        r"C:\miniconda3\Scripts\conda.exe",
        r"C:\ProgramData\miniconda3\Scripts\conda.exe",
        r"C:\ProgramData\Anaconda3\Scripts\conda.exe",
    ];
    candidates
        .iter()
        .find(|p| std::path::Path::new(p).exists())
        .map(|s| s.to_string())
}

