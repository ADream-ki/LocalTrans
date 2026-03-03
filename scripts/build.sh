#!/bin/bash
# LocalTrans Linux/macOS 打包脚本
# 使用 Docker 构建可执行文件

set -e

echo "========================================"
echo "LocalTrans EXE Builder"
echo "========================================"
echo

# 颜色定义
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# 检查 Docker
if ! command -v docker &> /dev/null; then
    echo -e "${RED}[ERROR] Docker 未安装${NC}"
    echo "请先安装 Docker: https://docs.docker.com/get-docker/"
    exit 1
fi

# 检查 Docker 是否运行
if ! docker info &> /dev/null; then
    echo -e "${RED}[ERROR] Docker 未运行${NC}"
    echo "请启动 Docker 后重试"
    exit 1
fi

echo -e "${GREEN}[INFO] Docker 环境检查通过${NC}"
echo

# 选择构建目标
echo "请选择构建目标:"
echo "  1. Linux 可执行文件"
echo "  2. 本地直接构建 (需要本地 Python 环境)"
echo
read -p "请输入选项 (1/2): " choice

case $choice in
    1)
        echo
        echo -e "${YELLOW}[INFO] 使用 Docker 构建 Linux 可执行文件...${NC}"
        echo
        
        # 构建 Docker 镜像
        echo "[INFO] 正在构建 Docker 镜像..."
        docker-compose -f docker/docker-compose.yml build build-linux
        
        # 执行打包
        echo "[INFO] 正在打包..."
        docker-compose -f docker/docker-compose.yml run --rm build-linux
        
        # 设置权限
        if [ -d "dist" ]; then
            chmod +x dist/localtrans 2>/dev/null || true
            chmod +x dist/localtrans-gui 2>/dev/null || true
        fi
        ;;
    2)
        echo
        echo -e "${YELLOW}[INFO] 使用本地 PyInstaller 构建...${NC}"
        echo
        
        # 检查 Python
        if ! command -v python3 &> /dev/null; then
            echo -e "${RED}[ERROR] Python 未安装${NC}"
            exit 1
        fi
        
        # 检查 PyInstaller
        if ! pip3 show pyinstaller &> /dev/null; then
            echo "[INFO] 正在安装 PyInstaller..."
            pip3 install pyinstaller
        fi
        
        # 安装项目依赖
        echo "[INFO] 正在安装项目依赖..."
        pip3 install -e ".[asr,mt,tts,gui]"
        
        # 执行打包
        echo "[INFO] 正在打包..."
        pyinstaller localtrans.spec --noconfirm
        ;;
    *)
        echo -e "${RED}[ERROR] 无效选项${NC}"
        exit 1
        ;;
esac

# 成功
echo
echo "========================================"
echo -e "${GREEN}[SUCCESS] 打包完成!${NC}"
echo "========================================"
echo
echo "输出目录: $(pwd)/dist"
echo
ls -la dist/ 2>/dev/null || echo "目录为空"
echo

# 清理
read -p "是否清理构建缓存? (y/n): " clean
if [ "$clean" = "y" ]; then
    rm -rf build
    echo "[INFO] 缓存已清理"
fi
