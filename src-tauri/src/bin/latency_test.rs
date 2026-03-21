//! 延迟测试程序 - 测试 ASR → 翻译 → TTS 各环节的延迟

use std::time::Instant;
use std::path::PathBuf;

fn main() -> anyhow::Result<()> {
    println!("=== LocalTrans 延迟测试 ===\n");
    
    // 测试 1: ASR 延迟
    test_asr_latency()?;
    
    // 测试 2: 翻译延迟
    test_translation_latency()?;
    
    // 测试 3: TTS 延迟
    test_tts_latency()?;
    
    println!("\n=== 测试完成 ===");
    Ok(())
}

fn test_asr_latency() -> anyhow::Result<()> {
    println!("--- 测试 ASR 延迟 ---");
    
    let model_dir = dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("LocalTrans")
        .join("models")
        .join("asr");
    
    println!("模型目录: {}", model_dir.display());
    
    if !model_dir.exists() {
        println!("模型目录不存在，跳过 ASR 测试");
        return Ok(());
    }
    
    // 检查可用的模型目录
    let model_dirs: Vec<PathBuf> = std::fs::read_dir(&model_dir)
        .map(|entries| {
            entries
                .filter_map(|e| e.ok())
                .filter(|e| e.path().is_dir())
                .map(|e| e.path())
                .collect()
        })?;
    
    println!("找到 {} 个模型目录", model_dirs.len());
    for dir in &model_dirs {
        println!("  - {}", dir.file_name().unwrap_or_default().to_string_lossy());
    }
    
    #[cfg(feature = "sherpa-backend")]
    {
        use localtrans_lib::asr::sherpa::SherpaAsrEngine;
        use localtrans_lib::asr::{AsrConfig, AsrEngine};
        
        // 尝试找到 Paraformer 模型
        let paraformer_dir = model_dirs.iter()
            .find(|d| d.to_string_lossy().contains("paraformer"))
            .cloned();
        
        if let Some(model_path) = paraformer_dir {
            println!("\n加载 Paraformer 模型: {}", model_path.display());
            
            let config = AsrConfig {
                model_path: model_path.clone(),
                ..Default::default()
            };
            
            let start = Instant::now();
            match SherpaAsrEngine::init(config) {
                Ok(mut engine) => {
                    let load_time = start.elapsed();
                    println!("模型加载时间: {:.2?}", load_time);
                    
                    // 测试推理延迟 - 使用 1 秒静音
                    let silence: Vec<f32> = vec![0.0; 16000];
                    
                    let mut inference_times = Vec::new();
                    for i in 0..5 {
                        let start = Instant::now();
                        match engine.transcribe(&silence, 16000) {
                            Ok(result) => {
                                let time = start.elapsed();
                                inference_times.push(time);
                                println!("推理 {} - 耗时: {:.2?}, 结果长度: {} 字符", 
                                    i + 1, time, result.text.len());
                            }
                            Err(e) => {
                                println!("推理 {} 失败: {}", i + 1, e);
                            }
                        }
                    }
                    
                    if !inference_times.is_empty() {
                        let avg: std::time::Duration = inference_times.iter().sum::<std::time::Duration>() / inference_times.len() as u32;
                        let min = inference_times.iter().min().unwrap();
                        let max = inference_times.iter().max().unwrap();
                        println!("\n推理延迟统计:");
                        println!("  平均: {:.2?}", avg);
                        println!("  最小: {:.2?}", min);
                        println!("  最大: {:.2?}", max);
                        println!("  RTF (实时因子): {:.3}", avg.as_secs_f32() / 1.0);
                    }
                }
                Err(e) => {
                    println!("模型加载失败: {}", e);
                }
            }
        } else {
            println!("未找到 Paraformer 模型");
        }
    }
    
    #[cfg(not(feature = "sherpa-backend"))]
    {
        println!("sherpa-backend feature 未启用，跳过 ASR 测试");
    }
    
    Ok(())
}

fn test_translation_latency() -> anyhow::Result<()> {
    println!("\n--- 测试翻译延迟 ---");
    
    #[cfg(feature = "loci-backend")]
    {
        use localtrans_lib::translation::LociTranslator;
        use localtrans_lib::translation::Translator;
        use std::path::Path;
        
        // Loci 模型路径
        let loci_path = Path::new("../../Loci");
        
        if !loci_path.exists() {
            println!("Loci 路径不存在: {}", loci_path.display());
            println!("跳过翻译测试");
            return Ok(());
        }
        
        println!("初始化 Loci 翻译引擎...");
        
        let start = Instant::now();
        match LociTranslator::init(loci_path) {
            Ok(mut translator) => {
                let load_time = start.elapsed();
                println!("翻译引擎加载时间: {:.2?}", load_time);
                
                // 测试翻译延迟
                let test_texts = vec![
                    ("你好，世界", "zho_Hans", "eng_Latn"),
                    ("这是一个测试句子，用于测量翻译延迟。", "zho_Hans", "eng_Latn"),
                    ("人工智能正在改变我们的生活方式。", "zho_Hans", "eng_Latn"),
                ];
                
                let mut times = Vec::new();
                for (i, (text, src, tgt)) in test_texts.iter().enumerate() {
                    let start = Instant::now();
                    match translator.translate(text, src, tgt) {
                        Ok(result) => {
                            let time = start.elapsed();
                            times.push(time);
                            println!("翻译 {} - 耗时: {:.2?}", i + 1, time);
                            println!("  原文: {}", text);
                            println!("  译文: {}", result.text);
                        }
                        Err(e) => {
                            println!("翻译 {} 失败: {}", i + 1, e);
                        }
                    }
                }
                
                if !times.is_empty() {
                    let avg: std::time::Duration = times.iter().sum::<std::time::Duration>() / times.len() as u32;
                    println!("\n翻译延迟统计:");
                    println!("  平均: {:.2?}", avg);
                    println!("  最小: {:.2?}", times.iter().min().unwrap());
                    println!("  最大: {:.2?}", times.iter().max().unwrap());
                }
            }
            Err(e) => {
                println!("翻译引擎加载失败: {}", e);
            }
        }
    }
    
    #[cfg(not(feature = "loci-backend"))]
    {
        println!("loci-backend feature 未启用，跳过翻译测试");
    }
    
    Ok(())
}

fn test_tts_latency() -> anyhow::Result<()> {
    println!("\n--- 测试 TTS 延迟 ---");
    
    let tts_dir = dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("LocalTrans")
        .join("models")
        .join("tts")
        .join("sherpa");
    
    println!("TTS 模型目录: {}", tts_dir.display());
    
    if !tts_dir.exists() {
        println!("TTS 模型目录不存在，跳过 TTS 测试");
        return Ok(());
    }
    
    // 检查可用的 TTS 模型
    let model_dirs: Vec<PathBuf> = std::fs::read_dir(&tts_dir)
        .map(|entries| {
            entries
                .filter_map(|e| e.ok())
                .filter(|e| e.path().is_dir())
                .map(|e| e.path())
                .collect()
        })
        .unwrap_or_default();
    
    println!("找到 {} 个 TTS 模型目录", model_dirs.len());
    for dir in &model_dirs {
        println!("  - {}", dir.file_name().unwrap_or_default().to_string_lossy());
    }
    
    #[cfg(feature = "sherpa-backend")]
    {
        // 查找 MeloTTS 模型
        let melo_dir = model_dirs.iter()
            .find(|d| d.to_string_lossy().contains("melo"))
            .cloned();
        
        if let Some(model_path) = melo_dir {
            println!("\n加载 MeloTTS 模型: {}", model_path.display());
            
            // 查找 model.onnx
            let model_file = model_path.join("model.onnx");
            let tokens_file = model_path.join("tokens.txt");
            let lexicon_file = model_path.join("lexicon.txt");
            let dict_dir = model_path.join("dict");
            
            if model_file.exists() && tokens_file.exists() {
                println!("模型文件: {}", model_file.display());
                println!("Tokens: {}", tokens_file.display());
                println!("Lexicon: {}", lexicon_file.display());
                
                // 使用 sherpa-rs VitsTts
                use sherpa_rs::tts::{VitsTts, VitsTtsConfig};
                use sherpa_rs::OnnxConfig;
                
                let vits_config = VitsTtsConfig {
                    model: model_file.to_string_lossy().to_string(),
                    tokens: tokens_file.to_string_lossy().to_string(),
                    lexicon: if lexicon_file.exists() { 
                        lexicon_file.to_string_lossy().to_string() 
                    } else { 
                        String::new() 
                    },
                    dict_dir: if dict_dir.exists() { 
                        dict_dir.to_string_lossy().to_string() 
                    } else { 
                        String::new() 
                    },
                    onnx_config: OnnxConfig {
                        provider: "cpu".to_string(),
                        num_threads: 2,
                        debug: false,
                    },
                    ..Default::default()
                };
                
                let start = Instant::now();
                let mut tts = VitsTts::new(vits_config);
                let load_time = start.elapsed();
                println!("模型加载时间: {:.2?}", load_time);
                
                // 测试合成延迟
                let test_texts = vec![
                    "你好",
                    "这是一个测试",
                    "人工智能正在改变我们的生活",
                ];
                
                let mut times = Vec::new();
                for (i, text) in test_texts.iter().enumerate() {
                    let start = Instant::now();
                    match tts.create(text, 0, 1.0) {
                        Ok(audio) => {
                            let time = start.elapsed();
                            times.push(time);
                            let duration_secs = audio.samples.len() as f32 / audio.sample_rate as f32;
                            println!("合成 {} - 耗时: {:.2?}, 音频时长: {:.2}s, RTF: {:.3}", 
                                i + 1, time, duration_secs, time.as_secs_f32() / duration_secs);
                        }
                        Err(e) => {
                            println!("合成 {} 失败: {}", i + 1, e);
                        }
                    }
                }
                
                if !times.is_empty() {
                    let avg: std::time::Duration = times.iter().sum::<std::time::Duration>() / times.len() as u32;
                    println!("\nTTS 延迟统计:");
                    println!("  平均: {:.2?}", avg);
                    println!("  最小: {:.2?}", times.iter().min().unwrap());
                    println!("  最大: {:.2?}", times.iter().max().unwrap());
                }
            } else {
                println!("缺少必要的模型文件");
                if !model_file.exists() { println!("  缺少: {}", model_file.display()); }
                if !tokens_file.exists() { println!("  缺少: {}", tokens_file.display()); }
            }
        } else {
            println!("未找到 MeloTTS 模型");
        }
    }
    
    #[cfg(not(feature = "sherpa-backend"))]
    {
        println!("sherpa-backend feature 未启用，跳过 TTS 测试");
    }
    
    Ok(())
}