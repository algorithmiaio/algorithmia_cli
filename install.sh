#!/bin/sh

set -e

main() {
    for cmd in curl tar gzip; do
        need_cmd $cmd
    done

    set_globals
    handle_command_line_args "$@"
}

set_globals() {
    pkg_version="1.0.1"
    default_prefix="${MIA_PREFIX-/usr/local}"
    base_url="https://github.com/anowell/mia/releases/download"
    completions_url="https://raw.githubusercontent.com/anowell/mia/master/completions"
}

set_architecture() {
    echo_verbose "detecting architecture"

    _ostype="$(uname -s)"
    _cputype="$(uname -m)"

    echo_verbose "uname -s reports: $_ostype"
    echo_verbose "uname -m reports: $_cputype"

    if [ "$_ostype" = Darwin -a "$_cputype" = i386 ]; then
        # Darwin `uname -s` lies
        if sysctl hw.optional.x86_64 | grep -q ': 1'; then
            local _cputype=x86_64
        fi
    fi

    case "$_ostype" in

        Linux)
            local _ostype=unknown-linux-gnu
            ;;

        Darwin)
            local _ostype=apple-darwin
            ;;

        MINGW* | MSYS* | CYGWIN*)
            local _ostype=pc-windows-gnu
            ;;

        *)
            err "unrecognized OS type: $_ostype"
            ;;

    esac

    case "$_cputype" in

        i386 | i486 | i686 | i786 | x86)
            local _cputype=i686
            ;;

        x86_64 | x86-64 | x64 | amd64)
            local _cputype=x86_64
            ;;

        *)
            err "unknown CPU type: $_cputype"

    esac

    # Detect 64-bit linux with 32-bit userland
    if [ $_ostype = unknown-linux-gnu -a $_cputype = x86_64 ]; then
        # $SHELL does not exist in standard 'sh', so probably only exists
        # if configure is running in an interactive bash shell. /usr/bin/env
        # exists *everywhere*.
        local _bin_to_probe="${SHELL-bogus_shell}"
        if [ ! -e "$_bin_to_probe" -a -e "/usr/bin/env" ]; then
            _bin_to_probe="/usr/bin/env"
        fi
        # $SHELL may be not a binary
        if [ -e "$_bin_to_probe" ]; then
            file -L "$_bin_to_probe" | grep -q "text" || _bin_to_probe="/usr/bin/env"
        fi
        if [ -e "$_bin_to_probe" ]; then
            file -L "$_bin_to_probe" | grep -q "x86[_-]64" || local _cputype=i686
        fi
    fi

    _pkg_arch="$_cputype-$_ostype"
    echo_verbose "architecture is $_pkg_arch"
}

set_shell() {
    if command -v getent > /dev/null 2>&1 ; then
        _shell=$(getent passwd $LOGNAME | cut -d: -f7)
        echo_verbose "detected user shell as '$_shell'"
    elif command -v dscl > /dev/null 2>&1 ; then
        _shell=$(dscl . -read /Users/$(id -un) UserShell | cut -f2 -d' ')
        echo_verbose "detected user shell as '$_shell'"
    else
        _shell=/bin/bash
        echo_verbose "guessing user shell as '$_shell'"
    fi
}

print_welcome_message() {
    local _prefix="$1"
    local _uninstall="$2"
    local _disable_sudo="$3"

    cat <<"EOF"

    /\
   /  \     Welcome to Algorithmia
  /    \    Command Line Tools
 /\    /\
/  \  /  \

EOF

#     if [ "$_disable_sudo" = false ]; then
#         if [ "$(id -u)" = 0 ]; then
#             cat <<EOF
# WARNING: This script appears to be running as root. While it will work
# correctly, it is not necessary to run this install script as root.

# EOF
#         fi
#     fi


    if [ "$_uninstall" = false ]; then
        cat <<EOF
This script will download mia and install it to $_prefix.

EOF
# You may install elsewhere by running this script with the --prefix=<path> option.
    else
        cat <<EOF
This script will uninstall the existing mia installation at $_prefix.

EOF
    fi

#     if [ "$_disable_sudo" = false ]; then
#         cat <<EOF
# The installer will run under 'sudo' and may ask you for your password. If you do
# not want the script to run 'sudo' then pass it the --disable-sudo flag.

# EOF
#     fi

}

# Migrate from pre-1.0 config location of ~/.algorithmia
# to the 1.0 location: ~/.algorithmia/config
migrate_config() {
    if [ -f ~/.algorithmia ]; then
        echo_verbose "migrating configuration..."
        mv ~/.algorithmia ~/.algorithmia.bak
        mkdir ~/.algorithmia
        mv ~/.algorithmia.bak ~/.algorithmia/config
    fi
}

handle_command_line_args() {
    local _prefix="$default_prefix"
    local _uninstall=false
    local _help=false
    local _disable_sudo=false

    case "$(uname -s)" in
        MINGW* | MSYS* | CYGWIN*)
            echo "Sorry, but this script doesn't currently support Windows. You can find the Windows install instructions here: https://algorithmia.com/developers/clients/cli#installing-the-algorithmia-cli"
            return
            ;;
        *)
    esac

    local _arg
    for _arg in "$@"; do
        case "${_arg%%=*}" in
            --uninstall )
                _uninstall=true
                ;;

            -h | --help )
                _help=true
                ;;

            --verbose)
                # verbose is a global flag
                flag_verbose=true
                ;;

            --disable-sudo)
                _disable_sudo=true
                ;;

            -y | --yes)
                # yes is a global flag
                flag_yes=true
                ;;

            --prefix)
                if is_value_arg "$_arg" "prefix"; then
                    _prefix="$(get_value_arg "$_arg")"
                fi
                ;;

            *)
                echo "Unknown argument '$_arg', displaying usage:"
                echo ${_arg%%=*}
                _help=true
                ;;

        esac

    done

    if [ "$_help" = true ]; then
        print_help
        exit 0
    fi

    print_welcome_message $_prefix $_uninstall $_disable_sudo
    set_architecture
    set_shell

    if [ "$_uninstall" = true ]; then
        uninstall_cli
    else
        install_cli
    fi

}

install_cli() {
    # download mia for platform
    local tmpdir=$(mktemp -d)
    cd $tmpdir
    echo_verbose "working directory: '$tmpdir'"

    local release_url="${base_url}/v${pkg_version}/algorithmia-v${pkg_version}-${_pkg_arch}.tar.gz"
    echo_verbose "downloading release tarball..."
    curl -sSfL "$release_url" -o "mia.tar.gz"

    # Remove old versions - this should be removed from future releases
    if which mia > /dev/null 2>&1; then
        echo_verbose "removing old version: $(which mia)"
        maybe_sudo rm -f $(which mia)
    fi

    echo_verbose "extracting release tarball..."
    tar -xzf mia.tar.gz

    # copy to $_prefix/bin
    echo_verbose "installing 'mia'..."
    maybe_sudo mkdir -p $_prefix/bin/
    maybe_sudo cp $tmpdir/mia $_prefix/bin/
    migrate_config

    # install completions
    echo_verbose "installing shell completions..."
    maybe_sudo mkdir -p /usr/local/share/zsh/site-functions/ || true
    maybe_sudo cp $tmpdir/completions/zsh/_mia /usr/local/share/zsh/site-functions/
    maybe_sudo mkdir -p /etc/bash_completion.d/
    maybe_sudo cp $tmpdir/completions/bash/mia /etc/bash_completion.d/

    if [ "$_shell" = "/bin/zsh" ]; then
        echo "Zsh completions should load in subsequent shells if your \$fpath contains '/usr/local/share/zsh/site-functions'. Reload completions in your current shell by running:"
        echo
        echo "    compinit"
        echo
    elif [ "$_shell" = "/bin/bash" ]; then
        echo "Bash completions should be automatically sourced in subsequent shells if 'bash-completion' is installed. You may manually source them by running:"
        echo
        echo "    source /etc/bash_completion.d/mia"
        echo
    fi

    echo "Installation complete! Run 'mia --help' to get started."
}

uninstall_cli() {
    maybe_sudo rm -f $_prefix/bin/mia || true
    maybe_sudo rm -f /usr/local/share/zsh/site-functions/_mia || true
    maybe_sudo rm -f /etc/bash_completion.d/mia || true

    # Remove old versions - this should be removed from future releases
    if which mia > /dev/null 2>&1; then
        maybe_sudo rm -f $(which mia)
    fi

    echo "mia uninstalled!"
}

print_help() {
echo '
Usage: install-mia.sh [--verbose]
Options:
     --uninstall                       Uninstall instead of install
     --help, -h                        Display usage information
'
}
    #  --prefix=<path>                   Install to a specific location (default /usr/local)
    #  --disable-sudo                    Do not run installer under sudo
    #  --yes, -y                         Disable the interactive mode

echo_verbose() {
    if [ "$flag_verbose" = true ]; then
        echo "$1"
    fi
}

err() {
    echo "$1" >&2
    exit 1
}

maybe_sudo() {
    local _is_windows=false
    case "$_ostype" in
        *windows*)
            _is_windows=true
            ;;
    esac

    local _is_root=false
    if [ $(id -u) = 0 ]; then
      _is_root=true
    fi

    if [ "$_disable_sudo" = true -o "$_is_windows" = true -o "$_is_root" = true ]; then
        "$@"
    else
        sudo "$@"
    fi
}

need_cmd() {
    if ! command -v "$1" > /dev/null 2>&1; then
        err "need '$1' (command not found)"
    fi
}

main "$@"
