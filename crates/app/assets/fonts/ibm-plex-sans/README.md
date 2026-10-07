# IBM Plex Sans

Regular and SemiBold are unchanged copies of the fonts bundled by Zed in
`assets/fonts/ibm-plex-sans`. Their upstream family is
[IBM Plex](https://github.com/IBM/plex).

Copyright © 2017 IBM Corp., with Reserved Font Name "Plex". These font files
remain under the SIL Open Font License 1.1; the full notice is in [OFL.txt](OFL.txt).
Distribute that notice with binaries containing the embedded fonts.

The Windows artifact includes `IBM-Plex-Sans-OFL.txt` alongside the executable.
The workspace's `cargo bundle -p crabdash` metadata includes `OFL.txt` in bundle
resources. For a manually distributed Linux executable, include this file in
the same archive or its installed documentation directory.

The fonts make GPUI's Linux default interface family available without a system
font installation. Custom interface preferences and the native macOS/Windows
default font selection remain unchanged.
