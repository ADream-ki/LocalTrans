"""
音频 I/O 压力测试脚本（生产前快检）

覆盖：
- 设备刷新稳定性
- I/O 参数构建稳定性
- ViewModel 重建稳定性
- 可选会话启动/停止循环
"""

from __future__ import annotations

import gc
import time
from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[1]
SRC = ROOT / "src"
if str(SRC) not in sys.path:
    sys.path.insert(0, str(SRC))

from PySide6.QtWidgets import QApplication

from localtrans.services.audio_device_service import AudioDeviceService
from localtrans.services.audio_io_service import AudioIOService
from localtrans.ui.viewmodels.audio_device_vm import AudioDeviceViewModel
from localtrans.ui.viewmodels.session_vm import SessionViewModel


def _run_session_cycles(app: QApplication, cycles: int = 3) -> tuple[int, int]:
    ok = 0
    fail = 0
    for _ in range(cycles):
        vm = SessionViewModel()
        try:
            vm.startSession()
            app.processEvents()
            time.sleep(0.5)
            if vm.isRunning:
                ok += 1
            else:
                fail += 1
        except Exception:
            fail += 1
        finally:
            try:
                vm.stopSession()
            except Exception:
                pass
            del vm
            app.processEvents()
            time.sleep(0.1)
    return ok, fail


def main() -> int:
    app = QApplication.instance() or QApplication([])

    device_service = AudioDeviceService.get_instance()
    io_service = AudioIOService.get_instance()

    started = time.time()
    baseline = len(gc.get_objects())

    # 1) 设备刷新
    for _ in range(80):
        device_service.refresh()

    # 2) 运行参数构建
    for _ in range(500):
        _ = io_service.build_runtime_options()

    # 3) VM 构建/销毁
    for _ in range(80):
        vm = AudioDeviceViewModel()
        vm.refresh()
        del vm
    gc.collect()

    # 4) 会话启动/停止两轮：区分冷启动常驻缓存和热重启增量
    cold_ok, cold_fail = _run_session_cycles(app, cycles=3)
    gc.collect()
    after_cold = len(gc.get_objects())

    warm_ok, warm_fail = _run_session_cycles(app, cycles=3)
    gc.collect()
    after_warm = len(gc.get_objects())

    elapsed = time.time() - started
    cold_delta = after_cold - baseline
    warm_delta = after_warm - after_cold
    print(
        f"stress_audio_io: elapsed={elapsed:.2f}s, "
        f"baseline={baseline}, after_cold={after_cold}, after_warm={after_warm}, "
        f"cold_delta={cold_delta}, warm_delta={warm_delta}, "
        f"cold_ok={cold_ok}, cold_fail={cold_fail}, "
        f"warm_ok={warm_ok}, warm_fail={warm_fail}"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
