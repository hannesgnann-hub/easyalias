# Linux Exports

This directory is reserved for built EasyAlias Linux packages.

Build on Linux from `../linux_src`:

```bash
npm install
npm run tauri build
```

Then copy the generated `.deb`, `.rpm`, and `.AppImage` files from `../linux_src/src-tauri/target/release/bundle/` into this directory.
