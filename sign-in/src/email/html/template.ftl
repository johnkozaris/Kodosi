<#--
  The frame of each e-mail: the ground and one card, as the sign-in pages, with the Kodosi mark at
  the head of the card. A mail program shows it with tables and with styles on each element. The
  mark has the colour of the card behind it, so it stays whole in a mail program that turns an
  e-mail dark. Outlook does not know a largest width or the padding of a link, so the card and
  the action have a second form in comments that only Outlook reads.

  `emailLayout` takes what is between its tags: Keycloak's own e-mails use it that way, so an
  e-mail that has no file here still gets this frame. `message` is an e-mail of Kodosi: what it
  is, one sentence, one action, and a small note. `when` is a time that the sentence holds: a
  line does not break inside it.
-->
<#macro emailLayout>
<!doctype html>
<html lang="${locale.language}" dir="${(ltr)?then('ltr','rtl')}">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<meta name="color-scheme" content="light only">
<meta name="supported-color-schemes" content="light only">
<style>
  .kds-card p { margin: 0 0 14px; font-size: 15px; line-height: 23px; color: #5f4838; }
  .kds-card a { color: #9a4516; font-weight: 600; }
</style>
</head>
<body style="margin:0;padding:0;background-color:#f0e9dc;">
<table role="presentation" width="100%" cellspacing="0" cellpadding="0" border="0" style="background-color:#f0e9dc;">
<tr>
<td align="center" style="padding:40px 16px 32px;">
  <!--[if mso]><table role="presentation" width="460" align="center" cellspacing="0" cellpadding="0" border="0"><tr><td><![endif]-->
  <table role="presentation" width="100%" cellspacing="0" cellpadding="0" border="0" style="max-width:460px;background-color:#fefbf5;border:1px solid #d9ccba;border-radius:22px;">
  <tr>
  <td class="kds-card" align="center" style="padding:30px 32px 34px;font-family:-apple-system,BlinkMacSystemFont,'Segoe UI',Roboto,'Noto Sans',Helvetica,Arial,sans-serif;font-size:15px;line-height:23px;color:#5f4838;">
    <#if url?? && url.resourcesUrl??>
    <img src="${url.resourcesUrl}/img/logo.png" width="168" height="48" alt="Kodosi" style="display:block;margin:0 auto 22px;border:0;outline:none;font-family:ui-monospace,'SF Mono',Menlo,Consolas,monospace;font-size:24px;line-height:48px;font-weight:800;color:#25160e;">
    <#else>
    <p style="margin:0 0 22px;font-family:ui-monospace,'SF Mono',Menlo,Consolas,monospace;font-size:24px;line-height:48px;font-weight:800;color:#25160e;">kodosi</p>
    </#if>
    <#nested>
  </td>
  </tr>
  </table>
  <!--[if mso]></td></tr></table><![endif]-->
  <p style="margin:18px 0 0;font-family:-apple-system,BlinkMacSystemFont,'Segoe UI',Roboto,'Noto Sans',Helvetica,Arial,sans-serif;font-size:12px;line-height:16px;color:#5f4838;">${realmName}</p>
</td>
</tr>
</table>
</body>
</html>
</#macro>

<#macro message title text="" when="" action="" link="" note="">
<@emailLayout>
    <h1 style="margin:0 0 10px;font-family:ui-monospace,'SF Mono','JetBrains Mono',Menlo,Consolas,monospace;font-size:21px;line-height:28px;font-weight:500;letter-spacing:-0.03em;color:#25160e;">${title}</h1>
    <#if text?has_content>
    <p style="margin:0 auto;max-width:340px;font-size:15px;line-height:23px;color:#5f4838;"><#if when?has_content><#list text?split(when) as part>${part}<#sep><span style="white-space:nowrap;">${when}</span></#list><#else>${text}</#if></p>
    </#if>
    <#if action?has_content && link?has_content>
    <table role="presentation" cellspacing="0" cellpadding="0" border="0" align="center" style="margin:26px auto 0;">
    <tr>
    <td align="center" bgcolor="#a95323" style="border-radius:999px;background-color:#a95323;mso-padding-alt:13px 28px;">
      <a href="${link}" style="display:inline-block;padding:13px 28px;font-size:15px;line-height:20px;font-weight:600;color:#fffaf3;text-decoration:none;border-radius:999px;">${action}</a>
    </td>
    </tr>
    </table>
    </#if>
    <#if note?has_content>
    <p style="margin:24px auto 0;max-width:360px;font-size:13px;line-height:19px;color:#5f4838;">${note}</p>
    </#if>
</@emailLayout>
</#macro>
