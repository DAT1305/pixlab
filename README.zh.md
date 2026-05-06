<p align="center">
  <img src="./src-tauri/icons/icon.png" width="112" alt="PixLab Desktop 图标" />
</p>

<h1 align="center">PixLab Desktop</h1>

<p align="center">
  用于像素画、spritesheet、动画预览和桌面宠物的桌面应用。
</p>

<p align="center">
  <a href="./README.md">English</a> ·
  <a href="./README.vi.md">Tiếng Việt</a>
</p>

## PixLab 是什么？

PixLab Desktop 将像素资源制作流程放进一个简洁的桌面应用：图片转像素风格、清理 sprite、切分 spritesheet、导出 GIF、生成动画，以及创建可互动的桌面宠物。

## 功能

- 将图片转换为清晰的像素风格输出。
- 清理 sprite，并去除简单背景。
- 切分 spritesheet、对齐帧、预览动画并导出 GIF。
- 通过文字描述或参考图片生成动画 sheet。
- 创建带动画的宠物，预览动作，并放到桌面上。
- 保存宠物历史并管理宠物图库。
- 导入和导出宠物文件，方便分享。
- 从 GitHub Releases 检查应用更新。

## 下载

安装包发布在 [GitHub Releases](https://github.com/DAT1305/pixlab/releases/latest)。

Windows release 完整构建时会包含 64 位和 32 位安装包。

## 从源码运行

需要：

- Node.js 20+
- Rust stable
- npm
- Windows Build Tools 或对应系统的构建 SDK

```bash
npm ci
npm run dev
```

## 构建

```bash
npm run build:windows      # Windows 64 位安装包
npm run build:windows:x86  # Windows 32 位安装包
npm run build:mac          # macOS app 和 DMG，需要在 macOS 上运行
```

## macOS 说明

公开 macOS 构建可能尚未 notarize。如果复制到 Applications 后被 Gatekeeper 拦截，请运行：

```bash
xattr -dr com.apple.quarantine "/Applications/PixLab Desktop.app"
```

## License

MIT
