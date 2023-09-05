### zcash params downloader

The `zcash params downloader` is a utility, similar to the script `zcutil/fetch-params.sh` used in the Zcash software, that is designed to download the Zcash Params files. These files are essential for the correct operation of the Zcash network and for maintaining transaction privacy and security.

The purpose of this utility is to retrieve the most up-to-date version of the Params files from a trusted source.

![Screenshot](./screenshot.png)

It performs the following steps:

1. The utility checks for the existence of the following files in the Zcash params directory directory:

- `sprout-proving.key`
- `sprout-verifying.key`
- `sapling-spend.params`
- `sapling-output.params`
- `sprout-groth16.params`

2. If any of the files from the list (1) are not found, the utility will download them from a trusted source (z.cash) in parallel mode. After the download is finished, it will verify the sha256 checksum of the downloaded file. If a file has already been downloaded and exists, it will only verify its hash without downloading it again. If the hash is incorrect, indicating an incomplete download or other issues, the user should delete the file and restart the utility.

#### Cross-compile for Windows

```
    rustup target add x86_64-pc-windows-gnu
    sudo apt-get install mingw-w64
    cargo build --target x86_64-pc-windows-gnu
```


