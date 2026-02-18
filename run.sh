#!/usr/bin/env bash
#
# MySQL Executor 启动脚本
# 支持 macOS、Linux 和 Windows (Git Bash/WSL/MSYS2)
# 自动检测并安装缺失的依赖
#

set -e

# 颜色定义
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# 打印带颜色的消息
info() { echo -e "${BLUE}[INFO]${NC} $1"; }
success() { echo -e "${GREEN}[SUCCESS]${NC} $1"; }
warn() { echo -e "${YELLOW}[WARN]${NC} $1"; }
error() { echo -e "${RED}[ERROR]${NC} $1"; exit 1; }

# 检测操作系统
detect_os() {
    case "$(uname -s)" in
        Darwin*)  OS="macos" ;;
        Linux*)   OS="linux" ;;
        CYGWIN*|MINGW*|MSYS*) OS="windows" ;;
        *)        OS="unknown" ;;
    esac
    info "检测到操作系统: $OS"
}

# 检测包管理器
detect_package_manager() {
    if [[ "$OS" == "macos" ]]; then
        if command -v brew &> /dev/null; then
            PKG_MGR="brew"
        else
            PKG_MGR="none"
        fi
    elif [[ "$OS" == "linux" ]]; then
        if command -v apt-get &> /dev/null; then
            PKG_MGR="apt"
        elif command -v yum &> /dev/null; then
            PKG_MGR="yum"
        elif command -v dnf &> /dev/null; then
            PKG_MGR="dnf"
        elif command -v pacman &> /dev/null; then
            PKG_MGR="pacman"
        elif command -v apk &> /dev/null; then
            PKG_MGR="apk"
        else
            PKG_MGR="none"
        fi
    elif [[ "$OS" == "windows" ]]; then
        if command -v choco &> /dev/null; then
            PKG_MGR="choco"
        elif command -v scoop &> /dev/null; then
            PKG_MGR="scoop"
        else
            PKG_MGR="none"
        fi
    fi
    info "包管理器: $PKG_MGR"
}

# 安装 Homebrew (macOS)
install_homebrew() {
    if [[ "$OS" == "macos" ]] && ! command -v brew &> /dev/null; then
        warn "未检测到 Homebrew，正在安装..."
        /bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"
        
        # 添加到 PATH
        if [[ -f "/opt/homebrew/bin/brew" ]]; then
            eval "$(/opt/homebrew/bin/brew shellenv)"
        elif [[ -f "/usr/local/bin/brew" ]]; then
            eval "$(/usr/local/bin/brew shellenv)"
        fi
        PKG_MGR="brew"
        success "Homebrew 安装完成"
    fi
}

# 检查并安装 Rust
check_rust() {
    if command -v rustc &> /dev/null && command -v cargo &> /dev/null; then
        RUST_VERSION=$(rustc --version | cut -d' ' -f2)
        info "Rust 已安装: $RUST_VERSION"
        return 0
    fi
    return 1
}

install_rust() {
    if check_rust; then
        return 0
    fi
    
    warn "未检测到 Rust，正在安装..."
    
    if [[ "$OS" == "windows" ]]; then
        if [[ "$PKG_MGR" == "choco" ]]; then
            choco install rustup.install -y
        elif [[ "$PKG_MGR" == "scoop" ]]; then
            scoop install rustup
        else
            warn "请手动安装 Rust: https://rustup.rs"
            error "无法自动安装 Rust"
        fi
    else
        # macOS / Linux 使用 rustup
        curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
        source "$HOME/.cargo/env"
    fi
    
    if check_rust; then
        success "Rust 安装完成"
    else
        error "Rust 安装失败"
    fi
}

# 检查并安装 Docker
check_docker() {
    if command -v docker &> /dev/null; then
        DOCKER_VERSION=$(docker --version | cut -d' ' -f3 | tr -d ',')
        info "Docker 已安装: $DOCKER_VERSION"
        return 0
    fi
    return 1
}

install_docker() {
    if check_docker; then
        return 0
    fi
    
    warn "未检测到 Docker，正在安装..."
    
    case "$OS" in
        macos)
            if [[ "$PKG_MGR" == "brew" ]]; then
                brew install --cask docker
                warn "请启动 Docker Desktop 应用程序"
            else
                warn "请手动安装 Docker Desktop: https://docker.com/products/docker-desktop"
            fi
            ;;
        linux)
            case "$PKG_MGR" in
                apt)
                    sudo apt-get update
                    sudo apt-get install -y docker.io docker-compose
                    sudo systemctl start docker
                    sudo systemctl enable docker
                    sudo usermod -aG docker "$USER"
                    ;;
                yum|dnf)
                    sudo $PKG_MGR install -y docker docker-compose
                    sudo systemctl start docker
                    sudo systemctl enable docker
                    sudo usermod -aG docker "$USER"
                    ;;
                pacman)
                    sudo pacman -S --noconfirm docker docker-compose
                    sudo systemctl start docker
                    sudo systemctl enable docker
                    sudo usermod -aG docker "$USER"
                    ;;
                apk)
                    sudo apk add docker docker-compose
                    sudo rc-update add docker boot
                    sudo service docker start
                    ;;
                *)
                    warn "请手动安装 Docker: https://docs.docker.com/engine/install/"
                    ;;
            esac
            ;;
        windows)
            if [[ "$PKG_MGR" == "choco" ]]; then
                choco install docker-desktop -y
            elif [[ "$PKG_MGR" == "scoop" ]]; then
                warn "请手动安装 Docker Desktop: https://docker.com/products/docker-desktop"
            fi
            ;;
    esac
    
    if check_docker; then
        success "Docker 安装完成"
    else
        warn "Docker 可能需要重启终端或系统后才能使用"
    fi
}

# 检查并安装 Docker Compose
check_docker_compose() {
    if command -v docker-compose &> /dev/null; then
        COMPOSE_VERSION=$(docker-compose --version | grep -oE '[0-9]+\.[0-9]+\.[0-9]+' | head -1)
        info "Docker Compose 已安装: $COMPOSE_VERSION"
        return 0
    elif docker compose version &> /dev/null; then
        COMPOSE_VERSION=$(docker compose version | grep -oE '[0-9]+\.[0-9]+\.[0-9]+' | head -1)
        info "Docker Compose (插件) 已安装: $COMPOSE_VERSION"
        return 0
    fi
    return 1
}

# 获取 docker compose 命令
get_compose_cmd() {
    if docker compose version &> /dev/null; then
        echo "docker compose"
    elif command -v docker-compose &> /dev/null; then
        echo "docker-compose"
    else
        echo ""
    fi
}

# 构建项目
build_project() {
    info "正在构建 Rust 项目..."
    
    if [[ ! -d "backend" ]]; then
        error "未找到 backend 目录，请确保在项目根目录运行此脚本"
    fi
    
    pushd backend > /dev/null
    cargo build --release
    popd > /dev/null
    
    success "项目构建完成"
}

# 运行测试
run_tests() {
    info "正在运行测试..."
    
    pushd backend > /dev/null
    cargo test
    popd > /dev/null
    
    success "测试完成"
}

# 使用 Docker 运行
run_docker() {
    info "正在使用 Docker 启动服务..."
    
    COMPOSE_CMD=$(get_compose_cmd)
    if [[ -z "$COMPOSE_CMD" ]]; then
        error "未找到 docker-compose 或 docker compose 命令"
    fi
    
    $COMPOSE_CMD up --build -d
    
    success "服务已启动"
    info "查看日志: $COMPOSE_CMD logs -f mysql-executor"
    info "停止服务: $COMPOSE_CMD down"
}

# 本地运行
run_local() {
    info "正在本地运行..."
    
    if [[ ! -f "backend/target/release/mysql-executor" ]]; then
        build_project
    fi
    
    # 检查配置文件
    if [[ ! -f "config/config.toml" ]]; then
        if [[ -f "config/config.toml.example" ]]; then
            warn "未找到配置文件，正在从示例创建..."
            cp config/config.toml.example config/config.toml
            warn "请编辑 config/config.toml 配置数据库连接信息"
            return 1
        else
            error "未找到配置文件模板"
        fi
    fi
    
    ./backend/target/release/mysql-executor
}

# 显示帮助
show_help() {
    echo ""
    echo "MySQL Executor 启动脚本"
    echo ""
    echo "用法: $0 [命令]"
    echo ""
    echo "命令:"
    echo "  install     安装所有依赖 (Rust, Docker)"
    echo "  build       构建 Rust 项目"
    echo "  test        运行测试"
    echo "  docker      使用 Docker Compose 启动服务"
    echo "  local       本地运行 (需要先配置数据库)"
    echo "  stop        停止 Docker 服务"
    echo "  logs        查看 Docker 日志"
    echo "  clean       清理构建产物"
    echo "  help        显示此帮助信息"
    echo ""
    echo "示例:"
    echo "  $0 install   # 安装依赖"
    echo "  $0 docker    # Docker 方式运行"
    echo "  $0 local     # 本地运行"
    echo ""
}

# 停止 Docker 服务
stop_docker() {
    info "正在停止 Docker 服务..."
    
    COMPOSE_CMD=$(get_compose_cmd)
    if [[ -z "$COMPOSE_CMD" ]]; then
        error "未找到 docker-compose 或 docker compose 命令"
    fi
    
    $COMPOSE_CMD down
    success "服务已停止"
}

# 查看日志
show_logs() {
    COMPOSE_CMD=$(get_compose_cmd)
    if [[ -z "$COMPOSE_CMD" ]]; then
        error "未找到 docker-compose 或 docker compose 命令"
    fi
    
    $COMPOSE_CMD logs -f
}

# 清理构建产物
clean_project() {
    info "正在清理构建产物..."
    
    if [[ -d "backend/target" ]]; then
        pushd backend > /dev/null
        cargo clean
        popd > /dev/null
    fi
    
    success "清理完成"
}

# 安装所有依赖
install_all() {
    info "正在检查并安装依赖..."
    
    detect_os
    detect_package_manager
    
    # macOS 先安装 Homebrew
    if [[ "$OS" == "macos" ]]; then
        install_homebrew
    fi
    
    # 安装 Rust
    install_rust
    
    # 安装 Docker (可选)
    echo ""
    read -p "是否安装 Docker? (y/N): " install_docker_choice
    if [[ "$install_docker_choice" =~ ^[Yy]$ ]]; then
        install_docker
    fi
    
    success "依赖安装完成"
}

# 默认运行：构建 + 测试
run_default() {
    info "========== MySQL Executor 自动构建与测试 =========="
    echo ""
    
    # 安装 Rust（如果需要）
    install_rust
    
    # 构建项目
    build_project
    
    # 运行测试
    run_tests
    
    echo ""
    success "========== 全部完成 =========="
    echo ""
    info "后续操作："
    info "  Docker 运行:  ./run.sh docker"
    info "  本地运行:     ./run.sh local"
    info "  查看帮助:     ./run.sh help"
}

# 主函数
main() {
    # 切换到脚本所在目录
    SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
    cd "$SCRIPT_DIR"
    
    detect_os
    detect_package_manager
    
    case "${1:-default}" in
        default)
            run_default
            ;;
        install)
            install_all
            ;;
        build)
            install_rust
            build_project
            ;;
        test)
            install_rust
            run_tests
            ;;
        docker)
            if ! check_docker; then
                install_docker
            fi
            check_docker_compose
            run_docker
            ;;
        local)
            install_rust
            run_local
            ;;
        stop)
            stop_docker
            ;;
        logs)
            show_logs
            ;;
        clean)
            clean_project
            ;;
        help|--help|-h)
            show_help
            ;;
        *)
            error "未知命令: $1\n运行 '$0 help' 查看帮助"
            ;;
    esac
}

main "$@"
