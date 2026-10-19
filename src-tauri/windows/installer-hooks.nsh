; Origami NSIS 安装钩子（tauri.conf.json → bundle.windows.nsis.installerHooks）。
;
; Tauri 的 per-user 默认安装目录写死为 %LOCALAPPDATA%\Origami，但 Win11 新版右键菜单
; 的稀疏包（windows-extension/）COM 代理进程看不到 %LOCALAPPDATA% 下的普通目录
; （Programs 子目录除外），装在那里右键菜单不会出现。因此改装到
; %LOCALAPPDATA%\Programs\Origami，并把旧位置的安装迁移过去。

; 本文件在 installer.nsi 定义 PRODUCTNAME 之前被 !include，故目录名直接写死。
!define ORIGAMI_OLD_INSTDIR "$LOCALAPPDATA\Origami"
!define ORIGAMI_NEW_INSTDIR "$LOCALAPPDATA\Programs\Origami"

; 有界面安装：在目录页之前换掉默认目录（含从旧安装恢复出的位置），让页面显示最终目录。
!define MUI_CUSTOMFUNCTION_GUIINIT OrigamiGuiInit
Function OrigamiGuiInit
  ${If} $INSTDIR == "${ORIGAMI_OLD_INSTDIR}"
    StrCpy $INSTDIR "${ORIGAMI_NEW_INSTDIR}"
  ${EndIf}
FunctionEnd

!macro NSIS_HOOK_PREINSTALL
  ; 静默安装（/S、更新器）不走 GUIInit，在这里兜底。
  ${If} $INSTDIR == "${ORIGAMI_OLD_INSTDIR}"
    StrCpy $INSTDIR "${ORIGAMI_NEW_INSTDIR}"
    SetOutPath $INSTDIR
  ${EndIf}

  ; 旧版装在 %LOCALAPPDATA%\Origami：先静默卸载（静默模式不删应用数据），避免留下两份
  ; 程序。卸载会删掉卸载项、文件关联和快捷方式，本安装段随后会重新写入。
  ; `_?=` 让卸载程序就地运行并同步等待，但它不会删除自身，故随后手动清理。
  ${If} ${FileExists} "${ORIGAMI_OLD_INSTDIR}\uninstall.exe"
    ExecWait '"${ORIGAMI_OLD_INSTDIR}\uninstall.exe" /S _?=${ORIGAMI_OLD_INSTDIR}'
    Delete "${ORIGAMI_OLD_INSTDIR}\uninstall.exe"
    RMDir "${ORIGAMI_OLD_INSTDIR}\icons\filetypes"
    RMDir "${ORIGAMI_OLD_INSTDIR}\icons"
    RMDir "${ORIGAMI_OLD_INSTDIR}"
  ${EndIf}
!macroend
