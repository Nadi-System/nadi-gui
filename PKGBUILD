# Maintainer: Gaurav Atreya <allmanpride@gmail.com>
pkgname=nadi-gui
pkgver=0.1
pkgrel=1
pkgdesc="Network Analysis and Data Integration GUI+IDE"
arch=('x86_64')
url="https://github.com/Nadi-System/${pkgname}"
license=('GPL3')
depends=('gcc-libs' 'gtk4')
makedepends=('rust' 'cargo' 'git')

build() {
	cargo build --release
}

package() {
    mkdir -p "$pkgdir/usr/bin"
    cp "../target/release/${pkgname}" "$pkgdir/usr/bin/${pkgname}"
    mkdir -p "$pkgdir/usr/share/applications/"
    cp "../org.zerosofts.NadiGui.desktop" "$pkgdir/usr/share/applications/org.zerosofts.NadiGui.desktop"
    mkdir -p "$pkgdir/usr/share/${pkgname}/icons/"
    cp "../resources/window.ui" "$pkgdir/usr/share/${pkgname}/"
    cp "../resources/resources.gresource.xml" "$pkgdir/usr/share/${pkgname}/"
    cp "../resources/icons/nadi.svg" "$pkgdir/usr/share/${pkgname}/icons/"
}
