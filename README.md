<p align="center">
  <img src="logo.png" alt="ALPack" width="300"/>
</p>

<h1 align="center">ALPack - Alpine Linux SandBox Packager</h1>
<h3 align="center">Manage Alpine Environments and Compile Static Binaries with Ease.</h3>

<p align="center">
  <img src="https://img.shields.io/badge/Platform-Linux-FCC624?&logo=linux&style=flat-square"/>
  <img src="https://img.shields.io/github/actions/workflow/status/LinuxProativo/ALPack/rust.yml?label=Test&style=flat-square&logo=github"/>
  <img src="https://img.shields.io/badge/Language-Rust_2024-orange?style=flat-square&logo=rust"/>
    <img src="https://img.shields.io/badge/RustC-1.85%2B-orange?style=flat-square&logo=rust"/>
    <img src="https://img.shields.io/badge/Build-Cargo-444444?style=flat-square&logo=rust&logoColor=white"/>
  <img src="https://img.shields.io/github/languages/code-size/LinuxProativo/ALPack?style=flat-square&logo=paperlessngx&label=Code%20Size"/>
  <img src="https://img.shields.io/github/repo-size/LinuxProativo/ALPack?style=flat-square&logo=paperlessngx&label=Repo%20Size"/>
  <img src="https://img.shields.io/badge/Binary-Static-success?style=flat-square&logo=chainlink&logoColor=white">
  <img src="https://img.shields.io/github/v/release/LinuxProativo/ALPack?label=Release&style=flat-square&logo=github"/>
  <img src="https://img.shields.io/github/license/LinuxProativo/ALPack?color=673ab7&label=License&style=flat-square&logo=opensourcehardware&logoColor=white"/>
</p>

## 🔍 Overview

**ALPack**  is a tool developed in Rust designed to create and manage multiple
**Alpine Linux rootfs** environments in a practical and reproducible manner.

It leverages tools such as `proot` or `bubblewrap (bwrap)` for environment isolation.
Specifically engineered to be distributed as a `fully static binary`, ALPack operates
without dynamic dependencies on the host system. This makes it ideal for CI/CD pipelines,
developer workstations, and isolated environments.

While its primary purpose is to facilitate **static binary compilation** by generating a
complete rootfs ready for build workflows and package development, ALPack also supports
parameters for configuring minimal environments. This flexibility extends the tool's
utility to a wide range of use cases beyond its core build focus.

## ✨ Features

- 📦 **Portable Rootfs**: Easily create and manage multiple portable Alpine Linux environments.

- ⚡ **Minimal Setup**: Fast and lightweight initialization of the Alpine ecosystem.

- 🧪 **Safe Sandboxing**: Secure environment for testing applications or restricted systems.

- 📁 **Multi-Directory Support**: Manage different rootfs instances independently.

- 💪 **Static Compilation**: Optimized for building static binaries using `musl` and Alpine's toolchain.

- 🛠️ **Build System**: Direct integration with **APKBUILDs** to simplify the packaging process.

- 🌌 **OverlayFS Support**: Layer changes over your rootfs to keep the base system clean.

- ⏳ **Ephemeral Sessions**: Automatically discard modifications after execution using the `-e` flag.

- 🔒 **Enhanced Isolation**: Use `-s` for secure, minimal mounting and maximum sandboxing.

- 💼 **Fully Portable**: Run anywhere without complex installation or host dependencies.

- 🔑 **Rootless Execution**: Full functionality without requiring real root privileges.

Lightweight, fast, and productivity-focused, ALPack bridges the gap between Alpine
Linux flexibility and secure isolated environments.

## 🚀 Usage

```bash
ALPack <parâmetro> [opções] [--] [ARGS...]
```

## ⚡ Basic Examples

Below is a practical workflow designed to be straightforward and reproducible.

### 1) Preparing the Rootfs Environment

Start by setting up a default environment. Use `--edge` if you intend to work with
APKBUILDs (highly recommended for Alpine-based builds).
```bash
$ ALPack setup --edge 
```

### 2) Running Commands Inside the Rootfs

Commands can be executed in multiple ways. Optionally, use `--` to distinguish ALPack
parameters from the guest commands. The `-c` parameter is also optional and helpful
in specific shell contexts.

```bash
$ ALPack run -- cat /etc/os-release$ ALPack run -c "cat /etc/os-release"
```

### 3) Mounting Source Code into the Rootfs

You can bind-mount your project directory from the host into the rootfs using
`--bind-args` as shown below:

```bash
$ ALPack run --b "--bind /home/user/project:/src" -c "cd /src && ./build.sh"
```

This is particularly useful when your project is located outside the standard
home directory.

### 4) Compiling with Static Linking Flags

Here are common examples for `C/C++` static builds:

```bash
# Force static linking for dependencies and the binary
PKG_CONFIG="pkg-config --static" \
CFLAGS="-static" \
LDFLAGS="-static" \
./configure --disable-shared --enable-static

# -all-static is optional but sometimes required for complex toolchains
make LDFLAGS="-all-static"
```

After compilation, verify if the binary is truly static:

```bash
$ ldd static-binary
## not a dynamic executable
## statically linked

$ file static-binary 
## ELF 64-bit LSB executable, x86-64, version 1 (SYSV), statically linked,... 
## ELF 64-bit LSB pie executable, x86-64, version 1 (SYSV), static-pie linked,...
```

## 📦 Optional Installation

You can install AlpineBox manually:

```bash
$ wget https://github.com/LinuxDicasPro/ALPack/releases/download/Continuous/ALPack
$ chmod +x ./ALPack
$ sudo mv ./ALPack /usr/bin/ALPack
```

## 🧪 Why Use ALPack for Static Binary Compilation?

Compiling static binaries offers a significant advantage when you need to distribute
an executable that does not depend on the host's `libc` or other dynamic libraries.
ALPack streamlines this process because:

- 📌 It provides a **ready-to-use and predictable Alpine rootfs** for compilation,
or a minimal environment where you maintain full control over the build toolchain;

- 📌 It isolates the build from the host system, ensuring that compilation is performed
without cluttering the host or relying on local toolchains;

- 📌 **ALPack itself is distributed as a static binary**, simplifying the portability of the
tool across any environment without the need to install multiple dependencies;

- 📌 Alpine Linux includes the necessary static libraries required for `C/C++` to compile
fully static binaries.

## 📜 MIT License

This repository has scripts created to be free software.  
Therefore, they can be distributed and/or modified within the terms of the ***MIT License***.

> ### See the [MIT License](LICENSE) file for details.

## 📬 Contact & Support

* 📧 **Email:** [m10ferrari1200@gmail.com](mailto:m10ferrari1200@gmail.com)
* 📧 **Email:** [contatolinuxdicaspro@gmail.com](mailto:contatolinuxdicaspro@gmail.com)
* 🛠️ **Issues:** [Report a bug](https://github.com/LinuxProativo/ALPack/issues)

<p align="center">
  <i>Developed with precision in Rust. 🦀</i>
</p>
