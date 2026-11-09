// Decode only the isolated fixture files selected by the test; never a user profile.
const fs=require('node:fs'),path=require('node:path'),{execFileSync}=require('node:child_process');
function readJson(file){
 const bytes=fs.readFileSync(file);
 if(!bytes.subarray(0,8).equals(Buffer.from('SQYSEAL\x01')))return JSON.parse(bytes.toString('utf8'));
 const file64=Buffer.from(path.resolve(file),'utf8').toString('base64');
 const script=`$ErrorActionPreference='Stop';Add-Type -AssemblyName System.Security;$p=[Text.Encoding]::UTF8.GetString([Convert]::FromBase64String('${file64}'));$b=[IO.File]::ReadAllBytes($p);$c=[Text.Encoding]::UTF8.GetBytes('Sqywein/storage/v1/'+[IO.Path]::GetFileName($p));$v=[Security.Cryptography.ProtectedData]::Unprotect($b[8..($b.Length-1)],$c,[Security.Cryptography.DataProtectionScope]::CurrentUser);[Console]::OutputEncoding=[Text.Encoding]::UTF8;[Console]::Write([Text.Encoding]::UTF8.GetString($v))`;
 return JSON.parse(execFileSync('powershell.exe',['-NoProfile','-EncodedCommand',Buffer.from(script,'utf16le').toString('base64')],{windowsHide:true,encoding:'utf8'}));
}
module.exports={readJson};
