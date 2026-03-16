"""
QML 前端测试脚本

验证 PySide6 + QML 界面加载和 ViewModel 注册。
"""

import os
import sys
from pathlib import Path

# 设置 Qt Quick Controls 样式
os.environ["QT_QUICK_CONTROLS_STYLE"] = "Material"

# 添加 src 到路径
src_path = Path(__file__).parent / "src"
sys.path.insert(0, str(src_path))

APP = None

def test_imports():
    """测试必要的导入"""
    print("=" * 60)
    print("1. 测试导入...")
    print("=" * 60)
    
    errors = []
    
    # 测试 PySide6
    try:
        from PySide6.QtGui import QGuiApplication
        from PySide6.QtCore import QUrl, QTimer
        from PySide6.QtQml import QQmlApplicationEngine, qmlRegisterType
        print("✅ PySide6 导入成功")
    except ImportError as e:
        errors.append(f"❌ PySide6 导入失败: {e}")
        print(errors[-1])
    
    # 测试 ViewModels
    try:
        from localtrans.ui.viewmodels.session_vm import SessionViewModel
        print("✅ SessionViewModel 导入成功")
    except ImportError as e:
        errors.append(f"❌ SessionViewModel 导入失败: {e}")
        print(errors[-1])
    
    try:
        from localtrans.ui.viewmodels.settings_vm import SettingsViewModel
        print("✅ SettingsViewModel 导入成功")
    except ImportError as e:
        errors.append(f"❌ SettingsViewModel 导入失败: {e}")
        print(errors[-1])
    
    try:
        from localtrans.ui.viewmodels.model_vm import ModelViewModel
        print("✅ ModelViewModel 导入成功")
    except ImportError as e:
        errors.append(f"❌ ModelViewModel 导入失败: {e}")
        print(errors[-1])
    
    try:
        from localtrans.ui.viewmodels.audio_device_vm import AudioDeviceViewModel
        print("✅ AudioDeviceViewModel 导入成功")
    except ImportError as e:
        errors.append(f"❌ AudioDeviceViewModel 导入失败: {e}")
        print(errors[-1])
    
    try:
        from localtrans.ui.viewmodels.platform_vm import PlatformViewModel
        print("✅ PlatformViewModel 导入成功")
    except ImportError as e:
        errors.append(f"❌ PlatformViewModel 导入失败: {e}")
        print(errors[-1])
    
    # 测试 Bridge
    try:
        from localtrans.ui.bridge import QtBridge
        print("✅ QtBridge 导入成功")
    except ImportError as e:
        errors.append(f"❌ QtBridge 导入失败: {e}")
        print(errors[-1])
    
    return len(errors) == 0, errors


def test_qml_files():
    """测试 QML 文件存在性"""
    print("\n" + "=" * 60)
    print("2. 测试 QML 文件...")
    print("=" * 60)
    
    qml_dir = Path(__file__).parent / "src" / "localtrans" / "ui" / "qml"
    
    required_files = [
        "Main.qml",
        "pages/SessionPage.qml",
        "pages/SettingsPage.qml",
        "pages/ModelPage.qml",
        "pages/DiagnosticsPage.qml",
        "components/ModelListView.qml",
        "components/DiagnosticsGroup.qml",
    ]
    
    errors = []
    for file in required_files:
        path = qml_dir / file
        if path.exists():
            print(f"✅ {file} 存在")
        else:
            errors.append(f"❌ {file} 不存在: {path}")
            print(errors[-1])
    
    return len(errors) == 0, errors


def test_qml_syntax():
    """测试 QML 文件语法（简单检查）"""
    print("\n" + "=" * 60)
    print("3. 测试 QML 语法...")
    print("=" * 60)
    
    qml_dir = Path(__file__).parent / "src" / "localtrans" / "ui" / "qml"
    
    errors = []
    qml_files = list(qml_dir.rglob("*.qml"))
    
    for qml_file in qml_files:
        try:
            content = qml_file.read_text(encoding='utf-8')
            
            # 检查基本结构
            if not content.strip().startswith(('import', '//', '/*', 'pragma')):
                print(f"⚠️ {qml_file.name}: 文件开头不是 import 语句")
            
            # 检查括号匹配
            open_braces = content.count('{')
            close_braces = content.count('}')
            if open_braces != close_braces:
                errors.append(f"❌ {qml_file.name}: 括号不匹配 ({open_braces} {{ vs {close_braces} }})")
                print(errors[-1])
            else:
                print(f"✅ {qml_file.name}: 语法检查通过")
                
        except Exception as e:
            errors.append(f"❌ {qml_file.name}: 读取失败: {e}")
            print(errors[-1])
    
    return len(errors) == 0, errors


def test_viewmodel_properties():
    """测试 ViewModel 属性"""
    print("\n" + "=" * 60)
    print("4. 测试 ViewModel 属性...")
    print("=" * 60)
    
    errors = []
    
    try:
        from localtrans.ui.viewmodels.session_vm import SessionViewModel
        from localtrans.ui.viewmodels.settings_vm import SettingsViewModel
        from localtrans.ui.viewmodels.model_vm import ModelViewModel
        from localtrans.ui.viewmodels.audio_device_vm import AudioDeviceViewModel
        from localtrans.ui.viewmodels.platform_vm import PlatformViewModel
        
        # SessionViewModel
        print("\n--- SessionViewModel ---")
        session_vm = SessionViewModel()
        print(f"  state: {session_vm.state}")
        print(f"  sourceLang: {session_vm.sourceLang}")
        print(f"  targetLang: {session_vm.targetLang}")
        print(f"  isRunning: {session_vm.isRunning}")
        print("✅ SessionViewModel 属性测试通过")
        
        # SettingsViewModel
        print("\n--- SettingsViewModel ---")
        settings_vm = SettingsViewModel()
        print(f"  sourceLang: {settings_vm.sourceLang}")
        print(f"  targetLang: {settings_vm.targetLang}")
        print(f"  asrBackend: {settings_vm.asrBackend}")
        print(f"  mtBackend: {settings_vm.mtBackend}")
        print("✅ SettingsViewModel 属性测试通过")
        
        # ModelViewModel
        print("\n--- ModelViewModel ---")
        model_vm = ModelViewModel()
        print(f"  asrModels: {model_vm.asrModels}")
        print(f"  mtModels: {model_vm.mtModels}")
        print(f"  ttsModels: {model_vm.ttsModels}")
        print(f"  lociModels: {model_vm.lociModels}")
        print(f"  modelDir: {model_vm.modelDir}")
        print("✅ ModelViewModel 属性测试通过")
        
        # AudioDeviceViewModel
        print("\n--- AudioDeviceViewModel ---")
        audio_vm = AudioDeviceViewModel()
        print(f"  getInputDevices(): {audio_vm.getInputDevices()[:2]}...")  # 只显示前2个
        print(f"  getOutputDevices(): {len(audio_vm.getOutputDevices())} 个设备")
        print(f"  selectedInput: {audio_vm.selectedInput}")
        print(f"  selectedOutput: {audio_vm.selectedOutput}")
        print("✅ AudioDeviceViewModel 属性测试通过")
        
        # PlatformViewModel
        print("\n--- PlatformViewModel ---")
        platform_vm = PlatformViewModel()
        diagnostics = platform_vm.getDiagnostics()
        print(f"  os: {diagnostics.get('os', 'N/A')}")
        print(f"  pythonVersion: {diagnostics.get('pythonVersion', 'N/A')}")
        print(f"  lociStatus: {diagnostics.get('lociStatus', 'N/A')}")
        print(f"  cudaDevice: {diagnostics.get('cudaDevice', 'N/A')}")
        print("✅ PlatformViewModel 属性测试通过")
        
    except Exception as e:
        errors.append(f"❌ ViewModel 属性测试失败: {e}")
        print(errors[-1])
        import traceback
        traceback.print_exc()
    
    return len(errors) == 0, errors


def test_qml_loading():
    """测试 QML 加载"""
    print("\n" + "=" * 60)
    print("5. 测试 QML 加载...")
    print("=" * 60)
    
    errors = []
    qml_warnings = []
    
    try:
        from PySide6.QtCore import QUrl, QTimer, qInstallMessageHandler, QtMsgType
        from PySide6.QtQml import QQmlApplicationEngine
        from PySide6.QtWidgets import QApplication

        # 收集 QML 警告和错误
        def message_handler(msg_type, context, msg):
            if msg_type in (QtMsgType.QtWarningMsg, QtMsgType.QtCriticalMsg, QtMsgType.QtFatalMsg):
                qml_warnings.append(f"[{msg_type.name}] {msg}")
        
        qInstallMessageHandler(message_handler)
        app = QApplication.instance()
        if app is None:
            errors.append("❌ QApplication 未初始化")
            return False, errors, qml_warnings
        
        # 创建引擎
        engine = QQmlApplicationEngine()
        
        # 设置上下文属性
        from localtrans.ui.viewmodels.session_vm import SessionViewModel
        from localtrans.ui.viewmodels.settings_vm import SettingsViewModel
        from localtrans.ui.viewmodels.model_vm import ModelViewModel
        from localtrans.ui.viewmodels.audio_device_vm import AudioDeviceViewModel
        from localtrans.ui.viewmodels.platform_vm import PlatformViewModel
        
        session_vm = SessionViewModel()
        settings_vm = SettingsViewModel()
        model_vm = ModelViewModel()
        audio_device_vm = AudioDeviceViewModel()
        platform_vm = PlatformViewModel()
        
        engine.rootContext().setContextProperty("sessionVM", session_vm)
        engine.rootContext().setContextProperty("settingsVM", settings_vm)
        engine.rootContext().setContextProperty("modelVM", model_vm)
        engine.rootContext().setContextProperty("audioDeviceVM", audio_device_vm)
        engine.rootContext().setContextProperty("platformVM", platform_vm)
        
        # 加载 QML
        qml_dir = Path(__file__).parent / "src" / "localtrans" / "ui" / "qml"
        main_qml = qml_dir / "Main.qml"
        
        print(f"加载 QML: {main_qml}")
        engine.load(QUrl.fromLocalFile(str(main_qml)))
        
        # 检查是否加载成功
        root_objects = engine.rootObjects()
        if root_objects:
            print(f"✅ QML 加载成功，根对象数量: {len(root_objects)}")
            print(f"  根对象类型: {root_objects[0].metaObject().className()}")
        else:
            errors.append("❌ QML 加载失败：没有根对象")
            print(errors[-1])
        
        # 打印 QML 警告
        if qml_warnings:
            print("\n⚠️ QML 警告/错误:")
            for warning in qml_warnings:
                print(f"  {warning}")
        
        # 清理
        engine.deleteLater()
        
    except Exception as e:
        errors.append(f"❌ QML 加载测试失败: {e}")
        print(errors[-1])
        import traceback
        traceback.print_exc()
    
    return len(errors) == 0, errors, qml_warnings


def main():
    """主测试函数"""
    global APP
    print("=" * 60)
    print("LocalTrans QML 前端测试")
    print("=" * 60)
    try:
        from PySide6.QtWidgets import QApplication
        APP = QApplication.instance() or QApplication([])
    except Exception as e:
        print(f"❌ QApplication 初始化失败: {e}")
        return False
    
    all_errors = []
    all_warnings = []
    
    # 1. 测试导入
    success, errors = test_imports()
    all_errors.extend(errors)
    
    # 2. 测试 QML 文件
    success, errors = test_qml_files()
    all_errors.extend(errors)
    
    # 3. 测试 QML 语法
    success, errors = test_qml_syntax()
    all_errors.extend(errors)
    
    # 4. 测试 ViewModel 属性
    success, errors = test_viewmodel_properties()
    all_errors.extend(errors)
    
    # 5. 测试 QML 加载
    success, errors, warnings = test_qml_loading()
    all_errors.extend(errors)
    all_warnings.extend(warnings)
    
    # 总结
    print("\n" + "=" * 60)
    print("测试总结")
    print("=" * 60)
    
    if all_errors:
        print(f"\n❌ 发现 {len(all_errors)} 个错误:")
        for error in all_errors:
            print(f"  {error}")
    else:
        print("\n✅ 所有测试通过!")
    
    if all_warnings:
        print(f"\n⚠️ 发现 {len(all_warnings)} 个警告:")
        for warning in all_warnings:
            print(f"  {warning}")
    
    return len(all_errors) == 0


if __name__ == "__main__":
    success = main()
    sys.exit(0 if success else 1)
