param([int]$BrowserProcessId,[string]$Action='names',[string]$Name='',[int]$Key=0)
$ErrorActionPreference='Stop'
$ProgressPreference='SilentlyContinue'
[Console]::OutputEncoding=New-Object Text.UTF8Encoding($false)
$OutputEncoding=[Console]::OutputEncoding
Add-Type -AssemblyName UIAutomationClient,UIAutomationTypes
Add-Type -TypeDefinition @'
using System;using System.Text;using System.Runtime.InteropServices;
public class SqyweinNativeQA {
 delegate bool E(IntPtr h,IntPtr l);
 [DllImport("user32.dll")]static extern bool EnumWindows(E e,IntPtr l);
 [DllImport("user32.dll")]static extern uint GetWindowThreadProcessId(IntPtr h,out uint p);
 [DllImport("user32.dll",CharSet=CharSet.Unicode)]static extern int GetClassNameW(IntPtr h,StringBuilder b,int n);
 [DllImport("user32.dll")]public static extern IntPtr SendMessageW(IntPtr h,uint m,IntPtr w,IntPtr l);
 [DllImport("user32.dll")]public static extern bool ShowWindow(IntPtr h,int n);
 public static IntPtr Find(uint pid,string name){IntPtr result=IntPtr.Zero;EnumWindows((h,l)=>{uint p;GetWindowThreadProcessId(h,out p);var s=new StringBuilder(80);GetClassNameW(h,s,80);if(p==pid&&s.ToString()==name){result=h;return false;}return true;},IntPtr.Zero);return result;}
}
'@
$window=[SqyweinNativeQA]::Find($BrowserProcessId,'SqyweinWindow')
if($window -eq [IntPtr]::Zero){throw 'Isolated browser window not found'}
if($Action -eq 'show'){[SqyweinNativeQA]::ShowWindow($window,4)|Out-Null;return}
if($Action -eq 'close'){[SqyweinNativeQA]::SendMessageW($window,0x10,[IntPtr]::Zero,[IntPtr]::Zero)|Out-Null;return}
if($Action -eq 'key'){[SqyweinNativeQA]::SendMessageW($window,0x100,[IntPtr]$Key,[IntPtr]::Zero)|Out-Null;return}
$root=[System.Windows.Automation.AutomationElement]::FromHandle($window)
if($Action -eq 'names' -or $Action -eq 'names-deep'){
 $scope=[System.Windows.Automation.TreeScope]::Descendants
 $items=$root.FindAll($scope,[System.Windows.Automation.Condition]::TrueCondition)
 ConvertTo-Json -InputObject @($items|ForEach-Object {$_.Current.Name}) -Compress;return
}
$condition=New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::NameProperty,$Name)
$element=$root.FindFirst([System.Windows.Automation.TreeScope]::Descendants,$condition)
if(!$element){throw "Accessible element not found: $Name"}
if($Action -eq 'invoke'){$element.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke()}
elseif($Action -eq 'selected'){$element.GetCurrentPattern([System.Windows.Automation.SelectionItemPattern]::Pattern).Current.IsSelected|ConvertTo-Json -Compress}
elseif($Action -eq 'selection'){$container=$element.GetCurrentPattern([System.Windows.Automation.SelectionItemPattern]::Pattern).Current.SelectionContainer;ConvertTo-Json -InputObject @($container.GetCurrentPattern([System.Windows.Automation.SelectionPattern]::Pattern).Current.GetSelection()|ForEach-Object{$_.Current.Name}) -Compress}
elseif($Action -eq 'select'){$element.GetCurrentPattern([System.Windows.Automation.SelectionItemPattern]::Pattern).Select()}
elseif($Action -eq 'focus'){$element.SetFocus()}
else{throw 'Unknown native QA action'}

