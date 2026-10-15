<#-- Keycloak's own e-mails without a file here call emailLayout too. -->
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
    <img src="${url.resourcesUrl}/img/wordmark.png" width="182" height="40" alt="Kodosi" style="display:block;margin:0 auto 24px;border:0;outline:none;font-family:ui-monospace,'SF Mono',Menlo,Consolas,monospace;font-size:24px;line-height:40px;font-weight:800;color:#25160e;">
    <#else>
    <p style="margin:0 0 24px;font-family:ui-monospace,'SF Mono',Menlo,Consolas,monospace;font-size:24px;line-height:40px;font-weight:800;color:#25160e;">Kodosi</p>
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
