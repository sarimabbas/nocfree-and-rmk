# NocFree RMK Companion

Companion helps you save the original software on your NocFree AND keyboard, install RMK, and change keys. It guides you through checks for both halves and all three connection modes. You can return to the original software with your saved backup.

![Choose the keyboard parts to update](docs/images/install-rmk.png)

## Download

| Computer | App |
| --- | --- |
| Mac with Apple Silicon | [Download for Mac](https://github.com/sarimabbas/nocfree-and-rmk/releases/latest/download/nocfree-rmk-companion-macos-arm64.zip) |
| Windows, 64-bit | [Download for Windows](https://github.com/sarimabbas/nocfree-and-rmk/releases/latest/download/nocfree-rmk-companion-windows-x86_64.zip) |
| Linux, 64-bit | [Download for Linux](https://github.com/sarimabbas/nocfree-and-rmk/releases/latest/download/nocfree-rmk-companion-linux-x86_64.tar.gz) |

On Mac, open the ZIP file and move **NocFree RMK Companion** to **Applications**.

On Windows, open the ZIP file, keep all its files together, then open **nocfree-companion.exe**. On Linux, open the archive and follow the included **README.txt**.

On Mac, you can also use Homebrew:

```sh
brew install --cask sarimabbas/tap/nocfree-rmk-companion
```

## Start here

1. Connect both keyboard halves and the dongle to your computer with USB.
2. Open Companion and select **Install RMK**.
3. Follow the instructions on each screen and click **Next** when you are ready.

Before it replaces the software on a part, Companion saves a backup automatically. Keep the USB cables connected until the app asks you to remove them.

## What works

- [x] Install RMK
- [x] Return to the original software with your saved backup
- [x] Type with a USB cable
- [x] Type with Bluetooth
- [x] Type with the dongle after installing RMK on it
- [x] Check both halves and all three connection modes with guided typing tests
- [x] Change keys
- [x] Change the backlight brightness with 16 levels
- [x] See an estimate of each half’s battery level

![Check the keyboard connections](docs/images/check-pairing.png)

The left switch selects the connection: **top = dongle**, **middle = USB**, **bottom = Bluetooth**. A USB cable can charge a half while it uses a wireless connection.

## Need help?

If you need help, use **Help → Export diagnostic logs** to save a ZIP file. Attach it to a [problem report](https://github.com/sarimabbas/nocfree-and-rmk/issues/new/choose).
