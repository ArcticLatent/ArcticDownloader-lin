@Echo off&&cd /D %~dp0&&chcp 65001 >nul
Title 'Update Easy-Install Modules' v0.1.0 by ivo
:: Pixaroma Community Edition ::

set warning=[33m
set     red=[91m
set   green=[92m
set  yellow=[93m
set    bold=[97m
set   reset=[0m

:: get the parrent folder ::
set "AutoRun_dir=%cd%"
cd ..\..\
set "parent_dir=%cd%"
cd %AutoRun_dir%

:: Copying the images into the ComfyUI\input folder ::

if exist ".\Helper-CEI\*.jpeg" move ".\Helper-CEI\*.jpeg" "..\..\ComfyUI\input\" >nul 2>&1
if exist ".\Helper-CEI\*.jpg" move ".\Helper-CEI\*.jpg" "..\..\ComfyUI\input\" >nul 2>&1
if exist ".\Helper-CEI\*.png" move ".\Helper-CEI\*.png" "..\..\ComfyUI\input\" >nul 2>&1

:: Updating the bat files ::
if exist ".\Helper-CEI\*.bat" move ".\Helper-CEI\*.bat" "..\..\" >nul 2>&1

:: usage: call :create_shortcut bat ico lnk ::
call :create_shortcut "Start ComfyUI.bat" "ComfyUI-EZi.ico" "ComfyUI-EZi.lnk"
call :create_shortcut "ComfyUI\output" "ComfyUI-EZi-output.ico" "ComfyUI-EZi output.lnk"
call :create_shortcut "Start ComfyUI SageAttention.bat" "ComfyUI-Sage.ico" "ComfyUI-SA.lnk"
call :create_shortcut "Start ComfyUI FlashAttention.bat" "ComfyUI-Flash.ico" "ComfyUI-FA.lnk"


:: Final Messages ::
echo.
echo %green%::::::::: Done. You can read what's new here: ::::::::::%reset%
echo %yellow%https://github.com/Tavris1/ComfyUI-Easy-Install/releases%reset%
echo.
if "%~1"=="" (
    echo %green%::::::::::::::::: Press any key to exit ::::::::::::::::%reset%&Pause>nul
    exit
)

exit /b

:: ================================ END ===========================

:create_shortcut

set "bat_name=%~1"
set "ico_name=%~2"
set "lnk_name=%~3"

:: Get real Desktop path ::
for /f "delims=" %%D in ('powershell -ExecutionPolicy Bypass -NoProfile -Command "[Environment]::GetFolderPath('Desktop')"') do set "DESKTOP=%%D"
:: Create a shortcut on the desktop ::
if exist ".\Helper-CEI\%ico_name%" if exist "..\..\%bat_name%" (
	echo %green%:::::: Creating desktop shortcut to%yellow% %bat_name%%reset%
	powershell -ExecutionPolicy Bypass -command "$s=(New-Object -ComObject WScript.Shell).CreateShortcut('%DESKTOP%\%lnk_name%'); $s.TargetPath='%parent_dir%\%bat_name%'; $s.WorkingDirectory='%parent_dir%\'; $s.IconLocation='%AutoRun_dir%\Helper-CEI\%ico_name%'; $s.Save();"
)
goto :eof