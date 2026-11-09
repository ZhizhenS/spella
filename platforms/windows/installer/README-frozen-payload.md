# Production build and repack

Run a full build with the configured Rust/MSVC environment:

```powershell
.\platforms\windows\installer\build-english-assist.ps1 -Installer -PythonPath C:\Python313\python.exe
```

Each production build command immediately copies its DLL/EXE results to a unique
`dist/.english-assist-freeze-<id>/` stage. Once the offline data and support files
are ready, the complete stage is hashed and published as `dist/english-assist/`,
before any Rust tests run. Later tests/examples may replace `target/` files without
changing this freeze. A previous frozen directory is preserved under
`dist/english-assist.previous-<id>/`; the build never deletes it or closes an app.

`build.json` schema 2 records the Cargo workspace source version, independent TIP
identity, every payload file's relative path/size/SHA256, the metadata binary hash
and Python compiler hash. The installed copy allows the actual DLL/EXE files to
be compared with the intended production freeze. Packaging does not establish
runtime or application compatibility.

Repack uses the verified freeze and needs no Rust/MSVC setup:

```powershell
.\platforms\windows\installer\repack-english-assist.ps1 -VerifyOnly
.\platforms\windows\installer\repack-english-assist.ps1
```

For an explicitly reviewed installer-script fix, retain the same native freeze:

```powershell
.\platforms\windows\installer\repack-english-assist.ps1 -RefreshScripts
```

`-RefreshScripts` validates the entire existing freeze first, updates only the five
runtime PowerShell scripts and their manifest records, and verifies every other
file stayed unchanged. A changed native file, old schema-1 manifest or source
version change requires a new full build. There is no fallback to `target/`.
To review a script refresh before compiling, combine `-RefreshScripts -VerifyOnly`.

ISCC requires explicit `PayloadRoot` and `MyAppVersion` macros, supplied by the
repack script. All installed files, setup icon and displayed license come from
that directory; only the installer definition comes from the checkout. Keep
version/hash verification in the repack entry when invoking ISCC. Do not run
ISCC `/O-` against the deliverable name: it has removed the existing setup file
in this environment.

Pure verification (temporary file fixtures, no compiler or installation):

```powershell
.\platforms\windows\installer\tests\frozen-payload.tests.ps1
```
