@echo off
REM LocalTrans Windows 打包脚本
REM 使用 Docker 构建可执行文件

setlocal EnableDelayedExpansion

echo ========================================
echo LocalTrans Windows EXE Builder
echo ========================================
echo.

REM 检查 Docker 是否可用
docker --version >nul 2>&1
if errorlevel 1 (
    echo [ERROR] Docker 未安装或未运行
    echo 请先安装 Docker Desktop: https://www.docker.com/products/docker-desktop
    exit /b 1
)

REM 检查 Docker Desktop 是否运行
docker info >nul 2>&1
if errorlevel 1 (
    echo [ERROR] Docker Desktop 未运行
    echo 请启动 Docker Desktop 后重试
    exit /b 1
)

echo [INFO] Docker 环境检查通过
echo.

REM 选择构建方式
echo 请选择构建方式:
echo   1. Windows 容器构建 (推荐，需要 Windows 容器支持)
echo   2. 本地 PyInstaller 直接构建 (需要本地 Python 环境)
echo.
set /p choice="请输入选项 (1/2): "

if "%choice%"=="1" goto docker_build
if "%choice%"=="2" goto local_build
echo [ERROR] 无效选项
exit /b 1

:docker_build
echo.
echo [INFO] 使用 Windows 容器构建...
echo [INFO] 注意: Windows 容器需要在 Docker Desktop 中启用
echo.

REM 切换到 Windows 容器模式
echo [INFO] 正在构建 Docker 镜像...
docker-compose -f docker/docker-compose.yml build build-windows

if errorlevel 1 (
    echo [ERROR] Docker 镜像构建失败
    echo.
    echo [TIP] 如果遇到 "Windows containers" 错误，请:
    echo       1. 右键点击 Docker Desktop 托盘图标
    echo       2. 选择 "Switch to Windows containers"
    echo       3. 或使用本地构建方式 (选项 2)
    exit /b 1
)

echo [INFO] 正在打包...
docker-compose -f docker/docker-compose.yml run --rm build-windows

if errorlevel 1 (
    echo [ERROR] 打包失败
    exit /b 1
)

goto success

:local_build
echo.
echo [INFO] 使用本地 PyInstaller 构建...
echo.

REM 检查 Python
python --version >nul 2>&1
if errorlevel 1 (
    echo [ERROR] Python 未安装
    echo 请安装 Python 3.10+ : https://www.python.org/downloads/
    exit /b 1
)

REM 检查 PyInstaller
pip show pyinstaller >nul 2>&1
if errorlevel 1 (
    echo [INFO] 正在安装 PyInstaller...
    pip install pyinstaller
)

REM 安装项目依赖
echo [INFO] 正在安装项目依赖...
pip install -e ".[asr,mt,tts,qml]"

REM 执行打包
echo [INFO] 正在打包...
pyinstaller localtrans.spec --noconfirm
pyinstaller localtrans-qml.spec --noconfirm

if errorlevel 1 (
    echo [ERROR] 打包失败
    exit /b 1
)

goto success

:success
echo.
echo ========================================
echo [SUCCESS] 打包完成!
echo ========================================
echo.
echo 输出目录: %cd%\dist
echo.
dir dist\*.exe 2>nul
echo.
echo 使用方法:
echo   - localtrans.exe     : 命令行版本
echo   - localtrans-qml.exe : 图形界面版本
echo.

REM 清理
echo [INFO] 是否清理构建缓存? (y/n)
set /p clean="请输入: "
if /i "!clean!"=="y" (
    rmdir /s /q build 2>nul
    echo [INFO] 缓存已清理
)

pause
