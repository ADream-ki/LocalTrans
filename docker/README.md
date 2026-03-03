# LocalTrans Docker 构建说明

## 快速开始

### Windows 用户

双击运行 `scripts\build.bat` 或在命令行执行：

```cmd
scripts\build.bat
```

### Linux/macOS 用户

```bash
chmod +x scripts/build.sh
./scripts/build.sh
```

## Docker Compose 命令

### 构建 Windows 版本 (需要 Windows 容器)

```bash
docker-compose -f docker/docker-compose.yml build build-windows
docker-compose -f docker/docker-compose.yml run --rm build-windows
```

### 构建 Linux 版本

```bash
docker-compose -f docker/docker-compose.yml build build-linux
docker-compose -f docker/docker-compose.yml run --rm build-linux
```

### 启动开发环境

```bash
docker-compose -f docker/docker-compose.yml up dev
```

### 运行测试

```bash
docker-compose -f docker/docker-compose.yml run --rm test
```

## 注意事项

### Windows 容器

Windows 容器构建需要：
1. Windows 10/11 Pro 或 Enterprise
2. Docker Desktop 并启用 Windows 容器模式
3. 右键 Docker Desktop 托盘图标 → "Switch to Windows containers"

### 输出目录

构建完成后，可执行文件位于 `dist/` 目录：
- `localtrans.exe` - 命令行版本
- `localtrans-gui.exe` - 图形界面版本

### 依赖说明

完整打包包含以下依赖（体积较大）：
- PyTorch（~2GB）
- transformers 模型支持
- PyQt6 GUI 库

如需精简，请修改 `localtrans.spec` 中的 `excludes` 列表。

## 手动打包

不使用 Docker，直接在本机打包：

```bash
# 安装 PyInstaller
pip install pyinstaller

# 安装项目依赖
pip install -e ".[asr,mt,tts,gui]"

# 执行打包
pyinstaller localtrans.spec --noconfirm
```
