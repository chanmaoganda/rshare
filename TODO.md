# rshare TODO

## High Priority

- [ ] **Android file picking** — Replace app-private `uploads/` dir scan with shared storage (e.g. `/sdcard/Download/`) or Android Intent via JNI. Currently requires `adb push`.
- [x] **Auto-connect error feedback** — Show "Connecting..." state, disable button during attempt, show error on failure.
- [ ] **Streaming downloads in app** — Currently downloads entire file into `Vec<u8>` before saving. Stream to disk with progress indicator for large files.
- [x] **Share link copyable** — Share URL now shown in selectable text field with close button.

## Medium Priority

- [ ] **Upload progress bar** — No progress indication during upload in the app.
- [x] **Delete confirmation** — Modal confirmation dialog before deleting files.
- [x] **CLI: persist delete tokens** — CLI now saves per-file delete tokens from uploads and uses them as fallback on delete.

## Lower Priority

- [x] **Multiple auto-refresh timers** — Old timer is cancelled before starting new one on reconnect.
- [x] **Server URL validation** — App validates URL scheme before attempting connection.
- [x] **Offline resilience** — Auto-refresh errors mark app as disconnected with "Disconnected" status.
