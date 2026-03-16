"""
CLI Controller - CLI 命令控制器

处理 CLI 命令并与 GUI 交互，支持 Agent 操作。
"""

import json
import sys
from typing import Optional, Dict, Any, Callable
from pathlib import Path

from PySide6.QtCore import QObject, Signal, Slot, QTimer
from loguru import logger


class CLIController(QObject):
    """
    CLI 命令控制器
    
    支持：
    - 标准输入命令
    - Unix Socket IPC
    - HTTP API（可选）
    """
    
    # 信号
    commandReceived = Signal(str, dict)  # command, args
    responseReady = Signal(dict)  # response
    
    # 单例
    _instance: Optional["CLIController"] = None
    
    def __new__(cls):
        if cls._instance is None:
            cls._instance = super().__new__(cls)
            cls._instance._initialized = False
        return cls._instance
    
    def __init__(self):
        if self._initialized:
            return
        
        super().__init__()
        
        self._commands: Dict[str, Callable] = {}
        self._socket_path: Optional[Path] = None
        self._socket_server = None
        self._tcp_port: int = 18765
        
        # 注册内置命令
        self._register_builtin_commands()
        
        self._initialized = True
        logger.info("CLI Controller 初始化完成")
    
    @classmethod
    def get_instance(cls) -> "CLIController":
        return cls()
    
    def _register_builtin_commands(self):
        """注册内置命令"""
        self._commands = {
            "start": self._cmd_start,
            "stop": self._cmd_stop,
            "swap": self._cmd_swap,
            "status": self._cmd_status,
            "set": self._cmd_set,
            "get": self._cmd_get,
            "navigate": self._cmd_navigate,
            "help": self._cmd_help,
            "quit": self._cmd_quit,
            "exit": self._cmd_quit,
        }
    
    def register_command(self, name: str, handler: Callable):
        """注册自定义命令"""
        self._commands[name] = handler
        logger.debug(f"注册命令: {name}")
    
    def execute(self, command: str, args: Dict[str, Any] = None) -> Dict[str, Any]:
        """执行命令"""
        args = args or {}
        
        if command not in self._commands:
            return {
                "success": False,
                "error": f"未知命令: {command}",
                "available_commands": list(self._commands.keys())
            }
        
        try:
            result = self._commands[command](args)
            response = {
                "success": True,
                "command": command,
                "result": result
            }
        except Exception as e:
            logger.exception(f"命令执行失败: {command}")
            response = {
                "success": False,
                "command": command,
                "error": str(e)
            }
        
        self.responseReady.emit(response)
        return response
    
    # === 内置命令处理器 ===
    
    def _cmd_start(self, args: Dict[str, Any]) -> Dict[str, Any]:
        """启动翻译会话"""
        from localtrans.ui.bridge import QtBridge
        bridge = QtBridge.get_instance()
        if bridge.session_vm:
            bridge.session_vm.startSession()
            return {"status": "started", "message": "翻译会话已启动"}
        return {"status": "error", "message": "SessionVM 不可用"}
    
    def _cmd_stop(self, args: Dict[str, Any]) -> Dict[str, Any]:
        """停止翻译会话"""
        from localtrans.ui.bridge import QtBridge
        bridge = QtBridge.get_instance()
        if bridge.session_vm:
            bridge.session_vm.stopSession()
            return {"status": "stopped", "message": "翻译会话已停止"}
        return {"status": "error", "message": "SessionVM 不可用"}
    
    def _cmd_swap(self, args: Dict[str, Any]) -> Dict[str, Any]:
        """交换源语言和目标语言"""
        from localtrans.ui.bridge import QtBridge
        bridge = QtBridge.get_instance()
        if bridge.session_vm:
            bridge.session_vm.swapLanguages()
            return {"status": "swapped"}
        return {"status": "error", "message": "SessionVM 不可用"}
    
    def _cmd_status(self, args: Dict[str, Any]) -> Dict[str, Any]:
        """获取当前状态"""
        from localtrans.ui.bridge import QtBridge
        bridge = QtBridge.get_instance()
        
        status = {
            "session": {
                "isRunning": False,
                "state": "idle"
            },
            "settings": {
                "sourceLang": "",
                "targetLang": "",
                "mtBackend": "",
                "asrBackend": "",
                "ttsEngine": ""
            }
        }
        
        if bridge.session_vm:
            status["session"]["isRunning"] = bridge.session_vm.isRunning
            status["session"]["state"] = bridge.session_vm.state
        
        if bridge.settings_vm:
            status["settings"]["sourceLang"] = bridge.settings_vm.sourceLang
            status["settings"]["targetLang"] = bridge.settings_vm.targetLang
            status["settings"]["mtBackend"] = bridge.settings_vm.mtBackend
            status["settings"]["asrBackend"] = bridge.settings_vm.asrBackend
            status["settings"]["ttsEngine"] = bridge.settings_vm.ttsEngine
        
        return status
    
    def _cmd_set(self, args: Dict[str, Any]) -> Dict[str, Any]:
        """设置配置项"""
        from localtrans.ui.bridge import QtBridge
        bridge = QtBridge.get_instance()
        
        if not bridge.settings_vm:
            return {"status": "error", "message": "SettingsVM 不可用"}
        
        changes = []
        if "sourceLang" in args:
            bridge.settings_vm.sourceLang = args["sourceLang"]
            changes.append(f"sourceLang={args['sourceLang']}")
        if "targetLang" in args:
            bridge.settings_vm.targetLang = args["targetLang"]
            changes.append(f"targetLang={args['targetLang']}")
        if "mtBackend" in args:
            bridge.settings_vm.mtBackend = args["mtBackend"]
            changes.append(f"mtBackend={args['mtBackend']}")
        if "asrBackend" in args:
            bridge.settings_vm.asrBackend = args["asrBackend"]
            changes.append(f"asrBackend={args['asrBackend']}")
        if "ttsEngine" in args:
            bridge.settings_vm.ttsEngine = args["ttsEngine"]
            changes.append(f"ttsEngine={args['ttsEngine']}")
        
        return {"status": "ok", "changes": changes}
    
    def _cmd_get(self, args: Dict[str, Any]) -> Dict[str, Any]:
        """获取配置项"""
        from localtrans.ui.bridge import QtBridge
        bridge = QtBridge.get_instance()
        
        if not bridge.settings_vm:
            return {"status": "error", "message": "SettingsVM 不可用"}
        
        key = args.get("key", "")
        if key:
            return {
                "key": key,
                "value": getattr(bridge.settings_vm, key, None)
            }
        
        return {
            "sourceLang": bridge.settings_vm.sourceLang,
            "targetLang": bridge.settings_vm.targetLang,
            "mtBackend": bridge.settings_vm.mtBackend,
            "asrBackend": bridge.settings_vm.asrBackend,
            "ttsEngine": bridge.settings_vm.ttsEngine
        }
    
    def _cmd_navigate(self, args: Dict[str, Any]) -> Dict[str, Any]:
        """导航到指定页面"""
        page = args.get("page", "session")
        self.commandReceived.emit("navigate", {"page": page})
        return {"status": "ok", "page": page}
    
    def _cmd_help(self, args: Dict[str, Any]) -> Dict[str, Any]:
        """显示帮助信息"""
        return {
            "commands": {
                "start": "启动翻译会话",
                "stop": "停止翻译会话",
                "swap": "交换源语言和目标语言",
                "status": "获取当前状态",
                "set": "设置配置项 (set sourceLang=en targetLang=zh)",
                "get": "获取配置项 (get [key])",
                "navigate": "导航到页面 (navigate page=session|settings|model|diagnostics)",
                "help": "显示帮助信息",
                "quit/exit": "退出应用"
            }
        }
    
    def _cmd_quit(self, args: Dict[str, Any]) -> Dict[str, Any]:
        """退出应用"""
        from PySide6.QtGui import QGuiApplication
        QGuiApplication.quit()
        return {"status": "quitting"}
    
    # === IPC 支持 ===
    
    def start_ipc_server(self, socket_path: Optional[str] = None):
        """启动 IPC 服务器（Unix Socket 或 Named Pipe）"""
        import platform
        
        if platform.system() == "Windows":
            # Windows 下同时启用 Named Pipe 和 AF_UNIX 回退通道，提升稳定性
            self._start_named_pipe()
            self._start_tcp_socket()
        else:
            self._start_unix_socket(socket_path)
            self._start_tcp_socket()
    
    def _start_unix_socket(self, socket_path: Optional[str]):
        """启动 Unix Socket 服务器"""
        import socket
        import threading
        
        if socket_path:
            self._socket_path = Path(socket_path)
        else:
            import tempfile
            self._socket_path = Path(tempfile.gettempdir()) / "localtrans.sock"
        
        def server_thread():
            if self._socket_path.exists():
                self._socket_path.unlink()
            
            server = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
            server.bind(str(self._socket_path))
            server.listen(5)
            server.settimeout(1)
            
            logger.info(f"IPC 服务器启动: {self._socket_path}")
            
            while True:
                try:
                    conn, _ = server.accept()
                    self._handle_connection(conn)
                except socket.timeout:
                    continue
                except Exception as e:
                    logger.error(f"IPC 服务器错误: {e}")
                    break
        
        thread = threading.Thread(target=server_thread, daemon=True)
        thread.start()
    
    def _start_named_pipe(self):
        """启动 Windows Named Pipe 服务器"""
        import threading
        
        def pipe_server_thread():
            try:
                import win32pipe
                import win32file
            except ImportError:
                logger.warning("win32pipe 不可用，Named Pipe IPC 未启动")
                return
            
            pipe_name = r"\\.\pipe\localtrans"
            
            while True:
                pipe = None
                try:
                    pipe = win32pipe.CreateNamedPipe(
                        pipe_name,
                        win32pipe.PIPE_ACCESS_DUPLEX,
                        win32pipe.PIPE_TYPE_MESSAGE | win32pipe.PIPE_READMODE_MESSAGE | win32pipe.PIPE_WAIT,
                        255,  # 最大实例数
                        65536, 65536,
                        0,
                        None
                    )
                    
                    win32pipe.ConnectNamedPipe(pipe, None)
                    
                    # 读取数据
                    try:
                        data = win32file.ReadFile(pipe, 65536)
                        if data and data[1]:
                            self._handle_pipe_data(data[1], pipe)
                    except Exception as e:
                        logger.debug(f"读取 Pipe 数据: {e}")
                        
                except Exception as e:
                    logger.error(f"Named Pipe 错误: {e}")
                finally:
                    if pipe:
                        try:
                            win32pipe.DisconnectNamedPipe(pipe)
                            win32file.CloseHandle(pipe)
                        except:
                            pass
        
        thread = threading.Thread(target=pipe_server_thread, daemon=True)
        thread.start()

    def _start_tcp_socket(self):
        """启动本地 TCP Socket 服务器（跨平台兜底 IPC）"""
        import socket
        import threading
        import tempfile

        endpoint_file = Path(tempfile.gettempdir()) / "localtrans_ipc_port.txt"

        def tcp_server_thread():
            server = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
            server.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
            bound = False

            for offset in range(0, 20):
                port = self._tcp_port + offset
                try:
                    server.bind(("127.0.0.1", port))
                    self._tcp_port = port
                    bound = True
                    break
                except OSError:
                    continue

            if not bound:
                logger.error("TCP IPC 启动失败：无可用端口")
                return

            try:
                endpoint_file.write_text(str(self._tcp_port), encoding="utf-8")
            except Exception as e:
                logger.warning(f"写入 TCP IPC 端口文件失败: {e}")

            server.listen(16)
            server.settimeout(1)
            logger.info(f"TCP IPC 服务器启动: 127.0.0.1:{self._tcp_port}")

            while True:
                try:
                    conn, _ = server.accept()
                    self._handle_connection(conn)
                except socket.timeout:
                    continue
                except Exception as e:
                    logger.error(f"TCP IPC 服务器错误: {e}")
                    break

        thread = threading.Thread(target=tcp_server_thread, daemon=True)
        thread.start()
    
    def _handle_connection(self, conn):
        """处理 Unix Socket 连接"""
        try:
            data = conn.recv(65536)
            if data:
                request = json.loads(data.decode("utf-8"))
                command = request.get("command", "")
                args = request.get("args", {})
                response = self.execute(command, args)
                conn.send(json.dumps(response).encode("utf-8"))
        except Exception as e:
            logger.error(f"处理连接错误: {e}")
        finally:
            conn.close()
    
    def _handle_pipe_data(self, data, pipe):
        """处理 Named Pipe 数据"""
        import win32file
        
        try:
            request = json.loads(data.decode("utf-8"))
            command = request.get("command", "")
            args = request.get("args", {})
            response = self.execute(command, args)
            win32file.WriteFile(pipe, json.dumps(response).encode("utf-8"))
        except Exception as e:
            logger.error(f"处理 Pipe 数据错误: {e}")
    
    @staticmethod
    def send_command(command: str, args: Dict[str, Any] = None) -> Dict[str, Any]:
        """发送命令到运行中的 GUI（静态方法，供外部调用）"""
        import socket
        import tempfile
        import platform
        
        args = args or {}
        request = {"command": command, "args": args}
        
        if platform.system() == "Windows":
            named_pipe_error = None
            try:
                import win32file
                # import win32pipe
                
                pipe_name = r"\\.\pipe\localtrans"
                pipe = win32file.CreateFile(
                    pipe_name,
                    win32file.GENERIC_READ | win32file.GENERIC_WRITE,
                    0, None,
                    win32file.OPEN_EXISTING,
                    0, None
                )
                win32file.WriteFile(pipe, json.dumps(request).encode("utf-8"))
                data = win32file.ReadFile(pipe, 65536)
                win32file.CloseHandle(pipe)
                return json.loads(data[1].decode("utf-8"))
            except Exception as e:
                named_pipe_error = str(e)

            # Named Pipe 失败时回退到 AF_UNIX socket
            unix_socket_error = None
            try:
                socket_path = Path(tempfile.gettempdir()) / "localtrans.sock"
                sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
                sock.connect(str(socket_path))
                sock.send(json.dumps(request).encode("utf-8"))
                data = sock.recv(65536)
                sock.close()
                return json.loads(data.decode("utf-8"))
            except Exception as e:
                unix_socket_error = str(e)
            # 继续回退到 TCP
            return CLIController._send_tcp_command(
                request,
                f"named_pipe={named_pipe_error}; unix_socket={unix_socket_error}",
            )

        try:
            socket_path = Path(tempfile.gettempdir()) / "localtrans.sock"
            sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
            sock.connect(str(socket_path))
            sock.send(json.dumps(request).encode("utf-8"))
            data = sock.recv(65536)
            sock.close()
            return json.loads(data.decode("utf-8"))
        except Exception as e:
            return CLIController._send_tcp_command(request, f"unix_socket={e}")

    @staticmethod
    def _send_tcp_command(request: Dict[str, Any], prefix_error: str = "") -> Dict[str, Any]:
        """通过本地 TCP 发送命令（兜底）。"""
        import socket
        import tempfile

        endpoint_file = Path(tempfile.gettempdir()) / "localtrans_ipc_port.txt"
        candidate_ports = []

        if endpoint_file.exists():
            try:
                candidate_ports.append(int(endpoint_file.read_text(encoding="utf-8").strip()))
            except Exception:
                pass

        candidate_ports.extend([18765 + i for i in range(0, 20)])

        for port in candidate_ports:
            try:
                with socket.create_connection(("127.0.0.1", port), timeout=1.5) as sock:
                    sock.send(json.dumps(request).encode("utf-8"))
                    data = sock.recv(65536)
                    return json.loads(data.decode("utf-8"))
            except Exception:
                continue

        error_msg = "tcp=无法连接到本地 GUI IPC 端口"
        if prefix_error:
            error_msg = f"{prefix_error}; {error_msg}"
        return {"success": False, "error": error_msg}


# CLI 入口
def main():
    """CLI 主入口"""
    import argparse
    
    parser = argparse.ArgumentParser(description="LocalTrans CLI")
    parser.add_argument("command", help="要执行的命令")
    parser.add_argument("--source", "-s", help="源语言")
    parser.add_argument("--target", "-t", help="目标语言")
    parser.add_argument("--backend", "-b", help="翻译后端")
    parser.add_argument("--page", "-p", help="页面")
    
    args = parser.parse_args()
    
    # 构建参数
    cmd_args = {}
    if args.source:
        cmd_args["sourceLang"] = args.source
    if args.target:
        cmd_args["targetLang"] = args.target
    if args.backend:
        cmd_args["mtBackend"] = args.backend
    if args.page:
        cmd_args["page"] = args.page
    
    # 发送命令
    result = CLIController.send_command(args.command, cmd_args)
    print(json.dumps(result, indent=2, ensure_ascii=False))
    
    return 0 if result.get("success") else 1


if __name__ == "__main__":
    sys.exit(main())
