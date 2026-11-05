; Transmutio 安装器拦截：检测旧版 FormatForge（ARP 显示名 + 旧 MSI UpgradeCode）。
; 只读取注册表，不自动卸载、不复制数据；命中即中止安装，避免两版并存。
;
; 注意：Installer\UpgradeCodes\<packed> 下的旧产品码是**值**（value）而不是子键，
; 实测 EnumRegKey 永远返回空，必须用 EnumRegValue 判断。为兼容其他写法，两者都查。

!macro ScanUninstallRoot ROOT ID
  StrCpy $R2 0
  loop_${ROOT}_${ID}:
    EnumRegKey $R3 ${ROOT} "Software\Microsoft\Windows\CurrentVersion\Uninstall" $R2
    StrCmp $R3 "" done_${ROOT}_${ID}
    ReadRegStr $R4 ${ROOT} "Software\Microsoft\Windows\CurrentVersion\Uninstall\$R3" "DisplayName"
    StrCmp $R4 "" next_${ROOT}_${ID}
    StrCpy $R5 $R4 11
    StrCmp $R5 "FormatForge" hit_${ROOT}_${ID} next_${ROOT}_${ID}
  hit_${ROOT}_${ID}:
    StrCpy $R9 "1"
    Goto done_${ROOT}_${ID}
  next_${ROOT}_${ID}:
    IntOp $R2 $R2 + 1
    Goto loop_${ROOT}_${ID}
  done_${ROOT}_${ID}:
!macroend

; 命中条件：该 UpgradeCode 键下存在任一产品码（值），或存在子键。
!macro CheckKeyPresent ROOT KEY
  ClearErrors
  EnumRegValue $R2 ${ROOT} "${KEY}" 0
  StrCmp $R2 "" +2
    StrCpy $R9 "1"
  ClearErrors
  EnumRegKey $R2 ${ROOT} "${KEY}" 0
  StrCmp $R2 "" +2
    StrCpy $R9 "1"
!macroend

Function CheckLegacyUpgradeKeys
  ; 旧 FormatForge LGPL {68CDE348-14E5-52EE-8A32-0972D9760143}
  ; 旧 FormatForge GPL  {D0BEF367-4F7D-57E2-AC29-60145E1DB4F0}
  ; 注册表使用打包后的 UpgradeCodes 键名（由上述 GUID 实测反查得到）。
  SetRegView 64
  !insertmacro CheckKeyPresent HKLM "Software\Classes\Installer\UpgradeCodes\843EDC865E41EE25A82390279D671034"
  !insertmacro CheckKeyPresent HKLM "Software\Classes\Installer\UpgradeCodes\763FEB0DD7F42E75CA920641E5D14B0F"
  !insertmacro CheckKeyPresent HKCU "Software\Classes\Installer\UpgradeCodes\843EDC865E41EE25A82390279D671034"
  !insertmacro CheckKeyPresent HKCU "Software\Classes\Installer\UpgradeCodes\763FEB0DD7F42E75CA920641E5D14B0F"
  SetRegView 32
  !insertmacro CheckKeyPresent HKLM "Software\Classes\Installer\UpgradeCodes\843EDC865E41EE25A82390279D671034"
  !insertmacro CheckKeyPresent HKLM "Software\Classes\Installer\UpgradeCodes\763FEB0DD7F42E75CA920641E5D14B0F"
  !insertmacro CheckKeyPresent HKCU "Software\Classes\Installer\UpgradeCodes\843EDC865E41EE25A82390279D671034"
  !insertmacro CheckKeyPresent HKCU "Software\Classes\Installer\UpgradeCodes\763FEB0DD7F42E75CA920641E5D14B0F"
  SetRegView lastused
FunctionEnd

; 入口：Tauri v2 NSIS 模板只认这四个 hook 宏名（见生成脚本里的
; !ifmacrodef NSIS_HOOK_PREINSTALL / _POSTINSTALL / _PREUNINSTALL / _POSTUNINSTALL）。
; PREINSTALL 位于 Section Install 最前面（SetOutPath 之后、第一个 File 之前），
; 命中即 Quit，不会写入任何文件。
!macro NSIS_HOOK_PREINSTALL
  ; 该段在 Section Install 内执行，$R2-$R5/$R9 可能被后续逻辑复用，用完必须还原。
  Push $R2
  Push $R3
  Push $R4
  Push $R5
  Push $R9
  StrCpy $R9 "0"
  SetRegView 64
  !insertmacro ScanUninstallRoot HKLM 64
  !insertmacro ScanUninstallRoot HKCU 64
  SetRegView 32
  !insertmacro ScanUninstallRoot HKLM 32
  !insertmacro ScanUninstallRoot HKCU 32
  SetRegView lastused
  Call CheckLegacyUpgradeKeys
  StrCmp $R9 "1" 0 legacy_not_found
    MessageBox MB_ICONSTOP|MB_OK "检测到已安装的旧版 FormatForge（LGPL 或 GPL）。$\r$\n$\r$\n请先卸载旧版（卸载时不要删除应用数据），再安装 ${PRODUCTNAME}。$\r$\n$\r$\n安装已中止，未写入任何文件。"
    Quit
  legacy_not_found:
  Pop $R9
  Pop $R5
  Pop $R4
  Pop $R3
  Pop $R2
!macroend
