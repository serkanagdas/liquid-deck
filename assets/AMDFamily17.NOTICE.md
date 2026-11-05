# Signed AMD sensor module

`AMDFamily17.bin` and `LpcIO.bin` are the unmodified signed PawnIO modules distributed by LibreHardwareMonitor v0.9.6.

SHA-256:
- `AMDFamily17.bin` `099dc01d6db97ea997fec4a461e191cc64b9d7ce47c9d2153c451c56c2adcf50`
- `LpcIO.bin` `3dcf8b2bc80ff642d97c4608511a818642b5bf315ff53df3df393d043e71d101`

Source: https://github.com/LibreHardwareMonitor/LibreHardwareMonitor/tree/v0.9.6/LibreHardwareMonitorLib/Resources/PawnIo

Wrapper and temperature conversion reference: https://github.com/LibreHardwareMonitor/LibreHardwareMonitor/blob/v0.9.6/LibreHardwareMonitorLib/Hardware/Cpu/Amd17Cpu.cs

Fan register logic ported from: https://github.com/LibreHardwareMonitor/LibreHardwareMonitor/tree/v0.9.6/LibreHardwareMonitorLib/Hardware/Motherboard/Lpc (LpcIO.cs, Nct677X.cs, IT87XX.cs)

Copyright LibreHardwareMonitor and contributors. Distributed under the Mozilla Public License 2.0; see `LibreHardwareMonitor.LICENSE`. Source is available at the URLs above. Liquid Deck uses only the signed module's `ioctl_read_smn` operation for the fixed Zen 4 temperature register, never fan or voltage control. The existing PawnIO driver verifies the module signature; no kernel driver is installed by this project.
