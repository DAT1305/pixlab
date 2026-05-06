<p align="center">
  <img src="./src-tauri/icons/icon.png" width="112" alt="Biểu tượng PixLab Desktop" />
</p>

<h1 align="center">PixLab Desktop</h1>

<p align="center">
  Ứng dụng desktop để làm pixel art, spritesheet, preview animation và pet trên màn hình.
</p>

<p align="center">
  <a href="./README.md">English</a> ·
  <a href="./README.zh.md">中文</a>
</p>

## PixLab Là Gì?

PixLab Desktop gom các công cụ xử lý asset pixel vào một app gọn: chuyển ảnh sang pixel art, làm sạch sprite, cắt spritesheet, xuất GIF, tạo animation và tạo pet tương tác trên màn hình.

## Tính Năng

- Chuyển ảnh thành pixel art sắc nét.
- Làm sạch sprite và xóa nền đơn giản.
- Cắt spritesheet, căn frame, xem chuyển động và xuất GIF.
- Tạo sheet animation từ mô tả hoặc ảnh tham chiếu.
- Tạo pet có animation, xem các hành động và gắn pet ra màn hình.
- Lưu lịch sử pet và quản lý thư viện pet.
- Import và export pet để chia sẻ.
- Kiểm tra bản cập nhật từ GitHub Releases.

## Tải App

Bản cài đặt được phát hành tại [GitHub Releases](https://github.com/DAT1305/pixlab/releases/latest).

Windows có bản 64-bit và 32-bit khi release được build đầy đủ.

## Chạy Từ Mã Nguồn

Cần có:

- Node.js 20+
- Rust stable
- npm
- Windows Build Tools hoặc bộ SDK tương ứng với hệ điều hành

```bash
npm ci
npm run dev
```

## Build

```bash
npm run build:windows      # installer Windows 64-bit
npm run build:windows:x86  # installer Windows 32-bit
npm run build:mac          # app và DMG cho macOS, chạy trên macOS
```

## Lưu Ý macOS

Bản macOS public có thể chưa được notarize. Nếu Gatekeeper chặn app sau khi copy vào Applications, chạy:

```bash
xattr -dr com.apple.quarantine "/Applications/PixLab Desktop.app"
```

## License

MIT
