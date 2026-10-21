# Maintainer: reekta92 <mdag.92988@protonmail.com>

pkgname=graf-rs-bin
pkgver=1.1.1
pkgrel=1
pkgdesc="Terminal-based force-directed graph visualizer for markdown wikilinks"
url="https://github.com/reekta92/graf"
license=("GPL-3.0-only")
arch=("x86_64" "aarch64")
depends=("glibc" "gcc-libs")
provides=("graf-rs")
conflicts=("graf-rs")
source_x86_64=("$url/releases/download/v$pkgver/graf-rs-$pkgver-x86_64-unknown-linux-gnu.tar.xz")
sha256sums_x86_64=("d01d4ae12c8b82a485ee9343983ae6028aa43789e9e65328861247de78fd9732")
source_aarch64=("$url/releases/download/v$pkgver/graf-rs-$pkgver-aarch64-unknown-linux-gnu.tar.xz")
sha256sums_aarch64=("26d17a712f823bd81e320e3410f28677816b0c986a8d7eb11c6a73df2f9787ea")

package() {
    install -Dm755 graf "$pkgdir/usr/bin/graf"
    install -Dm644 README.md "$pkgdir/usr/share/doc/$pkgname/README.md"
    install -Dm644 LICENSE "$pkgdir/usr/share/licenses/$pkgname/LICENSE"
}
