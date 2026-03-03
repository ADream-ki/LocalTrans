# Dockerfile for building Windows EXE
# 使用 Wine 在 Linux 上运行 PyInstaller for Windows

FROM ubuntu:22.04

ENV DEBIAN_FRONTEND=noninteractive
ENV WINEDEBUG=-all

# 安装 Wine 和依赖
RUN dpkg --add-architecture i386 && \
    apt-get update && \
    apt-get install -y \
    wine64 \
    wine32 \
    python3 \
    python3-pip \
    python3-venv \
    curl \
    wget \
    git \
    build-essential \
    && rm -rf /var/lib/apt/lists/*

# 设置工作目录
WORKDIR /build

# 复制项目文件
COPY . /build/

# 创建构建脚本
RUN echo '#!/bin/bash
set -e

echo "=== 安装 Python 依赖 ==="
pip3 install --upgrade pip
pip3 install pyinstaller
pip3 install -e ".[asr,mt,tts,gui]"

echo "=== 开始打包 ==="
pyinstaller localtrans.spec --noconfirm

echo "=== 打包完成 ==="
ls -la dist/
' > /build/build.sh && chmod +x /build/build.sh

CMD ["/bin/bash"]
