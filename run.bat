@echo off
chcp 65001 >nul
setlocal EnableDelayedExpansion

:: MySQL Executor 启动脚本 (Windows)
:: 支持自动检测并安装依赖

set "SCRIPT_DIR=%~dp0"
cd /d "%SCRIPT_DIR%"

:: 颜色代码 (Windows 10+)
set "RED=[91m"
set "GREEN=[92m"
set "YELLOW=[93m"
set "BLUE=[94m"
set "NC=[0m"

goto :main

:info
echo %BLUE%[INFO]%NC% %~1
goto :eof

:success
echo %GREEN%[SUCCESS]%NC% %~1
goto :eof

:warn
echo %YELLOW%[WARN]%NC% %~1
goto :eof

:error
echo %RED%[ERROR]%NC% %~1
exit /b 1

:check_rust
where rustc >nul 2>&1
if %errorlevel% equ 0 (
    for /f "tokens=2" %%v in ('rustc --version') do set "RUST_VERSION=%%v"
    call :info "Rust 已安装: !RUST_VERSION!"
    exit /b 0
)
exit /b 1

:install_rust
call :check_rust
if %errorlevel% equ 0 exit /b 0

call :warn "未检测到 Rust，正在安装..."

:: 检查是否有 winget
where winget >nul 2>&1
if %errorlevel% equ 0 (
    winget install Rustlang.Rustup -e --silent
    goto :rust_installed
)

:: 检查是否有 choco
where choco >nul 2>&1
if %errorlevel% equ 0 (
    choco install rustup.install -y
    goto :rust_installed
)

:: 检查是否有 scoop
where scoop >nul 2>&1
if %errorlevel% equ 0 (
    scoop install rustup
    goto :rust_installed
)

:: 手动下载安装
call :warn "正在下载 Rust 安装程序..."
curl -sSfL -o "%TEMP%\rustup-init.exe" https://win.rustup.rs/x86_64
if exist "%TEMP%\rustup-init.exe" (
    "%TEMP%\rustup-init.exe" -y
    del "%TEMP%\rustup-init.exe"
) else (
    call :error "下载 Rust 安装程序失败，请手动安装: https://rustup.rs"
)

:rust_installed
:: 刷新环境变量
set "PATH=%USERPROFILE%\.cargo\bin;%PATH%"
call :check_rust
if %errorlevel% equ 0 (
    call :success "Rust 安装完成"
) else (
    call :warn "Rust 安装完成，请重启终端后再运行"
)
exit /b 0

:check_docker
where docker >nul 2>&1
if %errorlevel% equ 0 (
    for /f "tokens=3" %%v in ('docker --version') do set "DOCKER_VERSION=%%v"
    set "DOCKER_VERSION=!DOCKER_VERSION:,=!"
    call :info "Docker 已安装: !DOCKER_VERSION!"
    exit /b 0
)
exit /b 1

:install_docker
call :check_docker
if %errorlevel% equ 0 exit /b 0

call :warn "未检测到 Docker"

:: 检查是否有 winget
where winget >nul 2>&1
if %errorlevel% equ 0 (
    call :info "正在使用 winget 安装 Docker Desktop..."
    winget install Docker.DockerDesktop -e --silent
    goto :docker_done
)

:: 检查是否有 choco
where choco >nul 2>&1
if %errorlevel% equ 0 (
    call :info "正在使用 Chocolatey 安装 Docker Desktop..."
    choco install docker-desktop -y
    goto :docker_done
)

call :warn "请手动安装 Docker Desktop: https://docker.com/products/docker-desktop"

:docker_done
call :check_docker
if %errorlevel% equ 0 (
    call :success "Docker 安装完成"
) else (
    call :warn "Docker 安装后请重启电脑"
)
exit /b 0

:build_project
call :info "正在构建 Rust 项目..."

if not exist "backend" (
    call :error "未找到 backend 目录"
)

pushd backend
cargo build --release
if %errorlevel% neq 0 (
    popd
    call :error "构建失败"
)
popd

call :success "项目构建完成"
exit /b 0

:run_tests
call :info "正在运行测试..."

pushd backend
cargo test
if %errorlevel% neq 0 (
    popd
    call :error "测试失败"
)
popd

call :success "测试完成"
exit /b 0

:run_docker
call :info "正在使用 Docker 启动服务..."

docker compose version >nul 2>&1
if %errorlevel% equ 0 (
    set "COMPOSE_CMD=docker compose"
) else (
    where docker-compose >nul 2>&1
    if %errorlevel% equ 0 (
        set "COMPOSE_CMD=docker-compose"
    ) else (
        call :error "未找到 docker compose 命令"
    )
)

%COMPOSE_CMD% up --build -d
if %errorlevel% neq 0 (
    call :error "Docker 启动失败"
)

call :success "服务已启动"
call :info "查看日志: %COMPOSE_CMD% logs -f mysql-executor"
call :info "停止服务: %COMPOSE_CMD% down"
exit /b 0

:run_local
call :info "正在本地运行..."

if not exist "backend\target\release\mysql-executor.exe" (
    call :build_project
)

if not exist "config\config.toml" (
    if exist "config\config.toml.example" (
        call :warn "未找到配置文件，正在从示例创建..."
        copy "config\config.toml.example" "config\config.toml" >nul
        call :warn "请编辑 config\config.toml 配置数据库连接信息"
        exit /b 1
    ) else (
        call :error "未找到配置文件模板"
    )
)

backend\target\release\mysql-executor.exe
exit /b 0

:stop_docker
call :info "正在停止 Docker 服务..."

docker compose version >nul 2>&1
if %errorlevel% equ 0 (
    docker compose down
) else (
    docker-compose down
)

call :success "服务已停止"
exit /b 0

:show_logs
docker compose version >nul 2>&1
if %errorlevel% equ 0 (
    docker compose logs -f
) else (
    docker-compose logs -f
)
exit /b 0

:clean_project
call :info "正在清理构建产物..."

if exist "backend\target" (
    pushd backend
    cargo clean
    popd
)

call :success "清理完成"
exit /b 0

:show_help
echo.
echo MySQL Executor 启动脚本
echo.
echo 用法: %~nx0 [命令]
echo.
echo 命令:
echo   install     安装所有依赖 (Rust, Docker)
echo   build       构建 Rust 项目
echo   test        运行测试
echo   docker      使用 Docker Compose 启动服务
echo   local       本地运行 (需要先配置数据库)
echo   stop        停止 Docker 服务
echo   logs        查看 Docker 日志
echo   clean       清理构建产物
echo   help        显示此帮助信息
echo.
echo 示例:
echo   %~nx0 install   # 安装依赖
echo   %~nx0 docker    # Docker 方式运行
echo   %~nx0 local     # 本地运行
echo.
exit /b 0

:run_default
call :info "========== MySQL Executor 自动构建与测试 =========="
echo.
call :install_rust
call :build_project
call :run_tests
echo.
call :success "========== 全部完成 =========="
echo.
call :info "后续操作："
call :info "  Docker 运行:  %~nx0 docker"
call :info "  本地运行:     %~nx0 local"
call :info "  查看帮助:     %~nx0 help"
exit /b 0

:install_all
call :info "正在检查并安装依赖..."
call :install_rust
echo.
set /p "install_docker_choice=是否安装 Docker? (y/N): "
if /i "!install_docker_choice!"=="y" (
    call :install_docker
)
call :success "依赖安装完成"
exit /b 0

:main
if "%~1"=="" goto :run_default
if /i "%~1"=="install" goto :install_all
if /i "%~1"=="build" (
    call :install_rust
    goto :build_project
)
if /i "%~1"=="test" (
    call :install_rust
    goto :run_tests
)
if /i "%~1"=="docker" (
    call :check_docker
    if %errorlevel% neq 0 call :install_docker
    goto :run_docker
)
if /i "%~1"=="local" (
    call :install_rust
    goto :run_local
)
if /i "%~1"=="stop" goto :stop_docker
if /i "%~1"=="logs" goto :show_logs
if /i "%~1"=="clean" goto :clean_project
if /i "%~1"=="help" goto :show_help
if /i "%~1"=="--help" goto :show_help
if /i "%~1"=="-h" goto :show_help

call :error "未知命令: %~1  运行 '%~nx0 help' 查看帮助"
