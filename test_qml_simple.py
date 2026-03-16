"""
简化的 QML 前端测试脚本
"""

import os
import sys
from pathlib import Path

os.environ["QT_QUICK_CONTROLS_STYLE"] = "Material"
sys.path.insert(0, str(Path(__file__).parent / "src"))

APP = None

def main():
    global APP
    print("=" * 60)
    print("LocalTrans QML 前端测试")
    print("=" * 60)
    try:
        from PySide6.QtWidgets import QApplication
        APP = QApplication.instance() or QApplication([])
    except Exception as e:
        print(f"  ❌ QApplication 初始化失败: {e}")
        return False
    
    all_passed = True
    
    # 1. 测试导入
    print("\n[1/5] 测试导入...")
    try:
        from PySide6.QtGui import QGuiApplication
        from PySide6.QtCore import QUrl
        from PySide6.QtQml import QQmlApplicationEngine
        print("  ✅ PySide6 导入成功")
    except ImportError as e:
        print(f"  ❌ PySide6 导入失败: {e}")
        return False
    
    # 2. 测试 ViewModel 导入
    print("\n[2/5] 测试 ViewModel 导入...")
    try:
        from localtrans.ui.viewmodels.session_vm import SessionViewModel
        from localtrans.ui.viewmodels.settings_vm import SettingsViewModel
        from localtrans.ui.viewmodels.model_vm import ModelViewModel
        from localtrans.ui.viewmodels.audio_device_vm import AudioDeviceViewModel
        from localtrans.ui.viewmodels.platform_vm import PlatformViewModel
        print("  ✅ 所有 ViewModel 导入成功")
    except ImportError as e:
        print(f"  ❌ ViewModel 导入失败: {e}")
        return False
    
    # 3. 测试 QML 文件
    print("\n[3/5] 测试 QML 文件存在性...")
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
    
    missing = []
    for file in required_files:
        path = qml_dir / file
        if not path.exists():
            missing.append(file)
    
    if missing:
        print(f"  ❌ 缺少文件: {missing}")
        all_passed = False
    else:
        print("  ✅ 所有 QML 文件存在")
    
    # 4. 测试 ViewModel 实例化
    print("\n[4/5] 测试 ViewModel 实例化...")
    try:
        session_vm = SessionViewModel()
        settings_vm = SettingsViewModel()
        model_vm = ModelViewModel()
        audio_vm = AudioDeviceViewModel()
        platform_vm = PlatformViewModel()
        
        print(f"  SessionViewModel: state={session_vm.state}, isRunning={session_vm.isRunning}")
        print(f"  SettingsViewModel: sourceLang={settings_vm.sourceLang}, targetLang={settings_vm.targetLang}")
        print(f"  ModelViewModel: modelDir={model_vm.modelDir}")
        print(f"  AudioDeviceViewModel: {len(audio_vm.getInputDevices())} input devices")
        print(f"  PlatformViewModel: lociStatus={platform_vm.getDiagnostics().get('lociStatus', 'N/A')}")
        print("  ✅ 所有 ViewModel 实例化成功")
    except Exception as e:
        print(f"  ❌ ViewModel 实例化失败: {e}")
        import traceback
        traceback.print_exc()
        all_passed = False
    
    # 5. 测试 QML 加载
    print("\n[5/5] 测试 QML 加载...")
    try:
        from PySide6.QtWidgets import QApplication
        app = QApplication.instance()
        if app is None:
            print("  ❌ QApplication 未初始化")
            return False
        engine = QQmlApplicationEngine()
        
        # 设置上下文属性
        engine.rootContext().setContextProperty("sessionVM", session_vm)
        engine.rootContext().setContextProperty("settingsVM", settings_vm)
        engine.rootContext().setContextProperty("modelVM", model_vm)
        engine.rootContext().setContextProperty("audioDeviceVM", audio_vm)
        engine.rootContext().setContextProperty("platformVM", platform_vm)
        
        # 加载 QML
        main_qml = qml_dir / "Main.qml"
        engine.load(QUrl.fromLocalFile(str(main_qml)))
        
        root_objects = engine.rootObjects()
        if root_objects:
            print(f"  ✅ QML 加载成功，根对象: {root_objects[0].metaObject().className()}")
        else:
            print("  ❌ QML 加载失败：没有根对象")
            all_passed = False
        
    except Exception as e:
        print(f"  ❌ QML 加载失败: {e}")
        import traceback
        traceback.print_exc()
        all_passed = False
    
    # 总结
    print("\n" + "=" * 60)
    if all_passed:
        print("✅ 所有测试通过!")
    else:
        print("❌ 部分测试失败")
    print("=" * 60)
    
    return all_passed


if __name__ == "__main__":
    success = main()
    sys.exit(0 if success else 1)
