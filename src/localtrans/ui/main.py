"""
QML GUI 入口

PySide6 + QML 跨平台桌面界面。
"""

import os
import sys
import tempfile
import traceback
from datetime import datetime
from pathlib import Path

from loguru import logger


def _write_startup_debug(message: str) -> None:
    """将启动期调试信息落到临时目录，便于定位打包环境异常。"""
    try:
        log_path = Path(tempfile.gettempdir()) / "localtrans_startup.log"
        with log_path.open("a", encoding="utf-8") as f:
            f.write(f"[{datetime.now().isoformat(timespec='seconds')}] {message}\n")
    except Exception:
        # 启动期调试不能影响主流程
        return


def _prepare_runtime_dirs() -> Path:
    """准备可写运行目录，避免受限环境下启动失败。"""
    candidates = [Path.home() / ".localtrans", Path.cwd() / ".localtrans"]
    base_dir = None

    for candidate in candidates:
        try:
            candidate.mkdir(parents=True, exist_ok=True)
            (candidate / "cache").mkdir(parents=True, exist_ok=True)
            (candidate / "hf_cache").mkdir(parents=True, exist_ok=True)
            base_dir = candidate
            break
        except Exception:
            continue

    if base_dir is None:
        # 最后兜底使用系统临时目录，避免当前目录不可写导致启动失败。
        base_dir = Path(tempfile.mkdtemp(prefix="localtrans-"))

    # 避免 hf-xet 在受限 HOME 下初始化失败
    os.environ.setdefault("HF_HUB_DISABLE_XET", "1")
    os.environ.setdefault("XDG_CACHE_HOME", str(base_dir / "cache"))
    os.environ.setdefault("HF_HOME", str(base_dir / "hf_cache"))
    os.environ.setdefault("HF_HUB_CACHE", str(base_dir / "hf_cache" / "hub"))
    # 覆盖应用目录到可写路径，避免打包环境下写入只读目录导致启动失败。
    os.environ.setdefault("LOCALTRANS_DATA_DIR", str(base_dir))
    os.environ.setdefault("LOCALTRANS_MODELS_DIR", str(base_dir / "models"))
    os.environ.setdefault("LOCALTRANS_LOGS_DIR", str(base_dir / "logs"))
    os.environ.setdefault("LOCALTRANS_CACHE_DIR", str(base_dir / "cache"))

    return base_dir


def run_qml_gui():
    """运行 QML GUI"""
    try:
        runtime_dir = _prepare_runtime_dirs()
        gui_log_dir = runtime_dir / "logs"
        gui_log_dir.mkdir(parents=True, exist_ok=True)
        gui_log = gui_log_dir / "gui-runtime.log"
        logger.add(str(gui_log), level="DEBUG", enqueue=False, backtrace=True, diagnose=True)
        logger.info(f"Runtime dir: {runtime_dir}")
        _write_startup_debug(f"runtime_dir={runtime_dir}")
        _write_startup_debug(f"gui_log={gui_log}")

        # 设置 Qt Quick Controls 样式 (Material 支持自定义)
        os.environ["QT_QUICK_CONTROLS_STYLE"] = "Basic"

        # 导入 PySide6
        try:
            from PySide6.QtWidgets import QApplication
            from PySide6.QtCore import QUrl
            from PySide6.QtQml import QQmlApplicationEngine
        except ImportError:
            logger.error("PySide6 未安装，请运行: pip install PySide6")
            print("PySide6 未安装，请运行: pip install PySide6")
            sys.exit(1)

        # 创建应用
        app = QApplication(sys.argv)
        app.setApplicationName("LocalTrans")
        app.setApplicationVersion("1.0.0")
        app.setOrganizationName("LocalTrans")

        def _excepthook(exc_type, exc_value, exc_tb):
            detail = "".join(traceback.format_exception(exc_type, exc_value, exc_tb))
            logger.error(f"Unhandled exception:\n{detail}")
            _write_startup_debug(f"Unhandled exception:\n{detail}")
            sys.__excepthook__(exc_type, exc_value, exc_tb)

        sys.excepthook = _excepthook

        # 初始化 Bridge
        from localtrans.ui.bridge import QtBridge

        bridge = QtBridge.get_instance()
        if not bridge.initialize():
            logger.error("Qt Bridge 初始化失败")
            _write_startup_debug("Qt Bridge 初始化失败")
            sys.exit(1)

        # 初始化 CLI Controller
        from localtrans.ui.cli_controller import CLIController

        cli_controller = CLIController.get_instance()
        logger.info("CLI Controller 已初始化")

        # 创建引擎
        engine = bridge.create_engine()

        # 加载 QML - 支持开发环境和打包后环境
        if getattr(sys, "frozen", False):
            base_path = Path(sys._MEIPASS)
            qml_dir = base_path / "localtrans" / "ui" / "qml"
        else:
            qml_dir = Path(__file__).parent / "qml"

        main_qml = qml_dir / "Main.qml"
        _write_startup_debug(f"qml={main_qml}")

        if not main_qml.exists():
            logger.error(f"QML 文件不存在: {main_qml}")
            print(f"QML 文件不存在: {main_qml}")
            _write_startup_debug(f"QML missing: {main_qml}")
            sys.exit(1)

        engine.load(QUrl.fromLocalFile(str(main_qml)))

        if not engine.rootObjects():
            logger.error("加载 QML 失败")
            _write_startup_debug("QML load failed: no root objects")
            sys.exit(1)

        # 获取根对象并连接 CLI 命令
        root_object = engine.rootObjects()[0]

        def on_cli_command(command, args):
            """处理 CLI 命令"""
            logger.info(f"CLI 命令: {command}, 参数: {args}")
            if hasattr(root_object, "executeCliCommand"):
                result = root_object.executeCliCommand(command, args)
                logger.debug(f"命令结果: {result}")

        cli_controller.commandReceived.connect(on_cli_command)

        # 启动 IPC 服务器（支持 Agent 操作）
        cli_controller.start_ipc_server()
        logger.info("IPC 服务器已启动")
        logger.info("QML GUI 启动成功")

        result = app.exec()
        bridge.cleanup()
        return result
    except Exception as exc:
        detail = f"{exc}\n{traceback.format_exc()}"
        _write_startup_debug(detail)
        logger.exception(f"QML GUI 启动异常: {exc}")
        raise


if __name__ == "__main__":
    run_qml_gui()
