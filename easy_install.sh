#!/bin/sh


if ! command -v rustc &> /dev/null; then

   # expected MD5 of Rust
   expected_md5="9457a47bb675d495b53d2ebbb757f63d"
   
   # temp installer file
   temp_file=$(mktemp)
   
   # download Rust installer
   curl --proto '=https' --tlsv1.2 -sSf -o "$temp_file" https://sh.rustup.rs
   
   # calculate the MD5 hash of the downloaded file
   calculated_md5=$(md5sum "$temp_file" | awk '{ print $1 }')
   
   # verify MD5 hash
   if [ "$expected_md5" != "$calculated_md5" ]; then
       echo "MD5 verification of Rust installer failed! This is likely a version change.\nPlease report, and install manually from:\nhttps://www.rust-lang.org/learn/get-started"
       rm "$temp_file"
       exit 1
   else
       echo "MD5 verified. Installing Rust..."
       sh "$temp_file"
   fi
   
   # remove the temporary file
   rm "$temp_file"
else
    echo "Rust is already installed."
fi
   
# install SciDataFlow
echo "Now installing SciDataFlow..."
cargo install scidataflow

