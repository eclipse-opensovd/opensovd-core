:: OpenSOVD Core build script
::
::

:: Don't show executed commands
@echo off

:: Variables are gone after termination of this script
setlocal EnableDelayedExpansion

:: Set required variables
set ROOT_DIR=%~dp0
set OUT_DIR=%ROOT_DIR%target\generated\openapi_client
:: Openapi generator has issues with backslashes in path
set OUT_DIR=%OUT_DIR:\=/%
set SPEC_FILE=%ROOT_DIR%sovd-interfaces\sovd-api.yaml
:: Openapi generator has issues with backslashes in path
set SPEC_FILE=%SPEC_FILE:\=/%
set GENERATOR_DIR=%ROOT_DIR%target
set GENERATOR_VERSION=7.10.0
set GENERATOR_JAR=%GENERATOR_DIR%\openapi-generator-cli-%GENERATOR_VERSION%.jar
set GENERATOR_URL=https://repo1.maven.org/maven2/org/openapitools/openapi-generator-cli/%GENERATOR_VERSION%/openapi-generator-cli-%GENERATOR_VERSION%.jar

:: Allowed commands
for %%a in (all clean codegen help validate) do set %%a=1
:: No command is given
if "%~1"=="" (
  goto :all
)
:: Progress command line parameter
if defined %1 (
  goto :%1 
) else (
  goto :help
)
exit /b 1

:all
  echo ==^> Starting SOVD build process...
  call :codegen
  if errorlevel 1 (
    exit /b 1
  )
  echo ==^> Building SOVD server...
  cargo build
  if errorlevel 1 (
    echo Error: Cargo build failed!
    exit /b 1
  )
  echo Build completed successfully.
  exit /b 0

:clean
  echo ==^> Cleaning generated rust code...
  rmdir /Q /S "%OUT_DIR%"
  echo Clean complete.
  exit /b 0

:codegen
  echo ==^> Starting SOVD code generation
  echo Project root: %ROOT_DIR%
  echo|set /p="OpenAPI Generator Version "
  java -jar "%GENERATOR_JAR%" version
  if errorlevel 1 (
    mkdir %GENERATOR_DIR%
    echo Downloading OpenAPI generator version %GENERATOR_VERSION%...
    curl "%GENERATOR_URL%" -o "%GENERATOR_JAR%"
    if errorlevel 1 (
      echo Error: Failed to download OpenAPI Generator CLI.
      exit /b 1
    )
    echo Downloaded openapi-generator-cli_%GENERATOR_VERSION%.jar to %GENERATOR_JAR%
  )
  echo Using existing OpenAPI Generator CLI: %GENERATOR_JAR%
  java -jar "%GENERATOR_JAR%" generate -i "%SPEC_FILE%" -g rust-server -o "%OUT_DIR%"
  echo Generated Rust code at %OUT_DIR%
  exit /b 0

:help
  echo Usage: %~n0 ^<command^>
  echo.
  echo Available commands:
  echo  all       Generate code and build sovd server
  echo  clean     Remove build artifacts and generated code
  echo  codegen   Run OpenAPI code generator
  echo  help      Show this help message
  echo  validate  Validate OpenAPI schema
  exit /b 0

:validate
  echo %SPEC_FILE%
  echo|set /p="OpenAPI Generator Version "
  java -jar "%GENERATOR_JAR%" version
  if errorlevel 1 (
    mkdir "%GENERATOR_DIR%"
    curl "%GENERATOR_URL%" -o "%GENERATOR_JAR%"
  )
  java -jar "%GENERATOR_JAR%" validate -i "%SPEC_FILE%"
  exit /b 0