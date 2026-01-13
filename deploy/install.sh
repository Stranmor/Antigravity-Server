#!/usr/bin/env bash
set -euo pipefail

# 权限检查
[[ $EUID -eq 0 ]] || { echo "错误: 需要 root 权限，请使用 sudo"; exit 1; }

# 如果通过 pipe 执行，创建目录并进入
if [[ ! -f "$0" ]] || [[ "$0" == "bash" ]]; then
    mkdir -p /opt/antigravity && cd /opt/antigravity
else
    cd "$(dirname "$0")"
fi

REPO="lbjlaq/Antigravity-Manager"

echo "[1/4] 安装依赖..."
if command -v apt-get &>/dev/null; then
    apt-get update -qq || echo "警告: apt-get update 部分失败，继续尝试安装..."
    apt-get install -y -qq xvfb libharfbuzz0b libwebkit2gtk-4.1-0 libgtk-3-0 wget curl jq || {
        echo "错误: 依赖安装失败，请检查镜像源或手动安装"
        exit 1
    }
elif command -v dnf &>/dev/null; then
    dnf install -y -q xorg-x11-server-Xvfb harfbuzz webkit2gtk4.1 gtk3 wget curl jq
elif command -v yum &>/dev/null; then
    yum install -y -q xorg-x11-server-Xvfb harfbuzz webkit2gtk3 gtk3 wget curl jq
else
    echo "错误: 不支持的包管理器，请手动安装: xvfb, libwebkit2gtk, libgtk-3, wget, curl, jq"
    exit 1
fi

echo "[2/4] 下载程序..."
VERSION=$(curl -sfS "https://api.github.com/repos/${REPO}/releases/latest" | jq -r '.tag_name // empty' | sed 's/^v//')
[[ -n "$VERSION" ]] || { echo "错误: 无法获取版本号，可能是 GitHub API 限流"; exit 1; }

wget -q --show-progress -O antigravity.AppImage \
    "https://github.com/${REPO}/releases/latest/download/Antigravity.Tools_${VERSION}_amd64.AppImage"
chmod +x antigravity.AppImage
echo "$VERSION" > .version

echo "[3/4] 初始化目录..."
mkdir -p .antigravity_tools/accounts logs
[[ -f .antigravity_tools/gui_config.json ]] || \
    echo '{"proxy":{"enabled":true,"auto_start":true,"port":8045}}' > .antigravity_tools/gui_config.json

echo "[4/4] 安装服务..."
cat > /etc/systemd/system/antigravity.service << EOF
[Unit]
Description=Antigravity Tools
After=network.target

[Service]
Type=simple
WorkingDirectory=$(pwd)
Environment="HOME=$(pwd)"
ExecStart=/usr/bin/xvfb-run -a $(pwd)/antigravity.AppImage
Restart=always
StandardOutput=append:$(pwd)/logs/app.log
StandardError=append:$(pwd)/logs/app.log

[Install]
WantedBy=multi-user.target
EOF

systemctl daemon-reload
systemctl enable antigravity
systemctl start antigravity

sleep 3
if systemctl is-active --quiet antigravity; then
    echo ""
    echo "========================================"
    echo "  部署完成！"
    echo "========================================"
    echo ""
    echo "  版本: v${VERSION}"
    echo "  目录: $(pwd)"
    echo "  端口: 8045（需同步账号后生效）"
    echo ""
    echo "  下一步："
    echo "  1. 在本地执行 sync.sh 同步账号："
    echo "     curl -O https://raw.githubusercontent.com/lbjlaq/Antigravity-Manager/main/deploy/sync.sh"
    echo "     chmod +x sync.sh && ./sync.sh user@$(hostname -I | awk '{print $1}')"
    echo ""
    echo "  常用命令："
    echo "     systemctl status antigravity    # 查看状态"
    echo "     systemctl restart antigravity   # 重启服务"
    echo "     tail -f $(pwd)/logs/app.log     # 查看日志"
    echo ""
else
    echo "启动失败，查看 $(pwd)/logs/app.log"
    exit 1
fi
