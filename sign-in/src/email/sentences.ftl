<#function moment>
<#return event.date?string("yyyy-MM-dd HH:mm zzz")>
</#function>

<#-- Keycloak reads a missing key as a message format, so only plain names are looked up. -->
<#function wordsOf prefix name>
<#if !name?matches("[A-Za-z0-9_.-]+")><#return ""></#if>
<#local words = msg(prefix + name)>
<#if words == prefix + name><#return ""></#if>
<#return words>
</#function>

<#function eventText key>
<#return msg(key, moment(), event.ipAddress)>
</#function>

<#function wayText withKind plain>
<#local kind = wordsOf("kdsWay.", event.getDetail("credential_type")!"")>
<#if kind?has_content><#return msg(withKind, kind, moment(), event.ipAddress)></#if>
<#return msg(plain, moment(), event.ipAddress)>
</#function>

<#function setupText>
<#local steps = []>
<#list requiredActions![] as step>
<#local words = wordsOf("requiredAction.", step)>
<#if words?has_content && !steps?seq_contains(words)><#local steps = steps + [words]></#if>
</#list>
<#if steps?size gt 0><#return msg("kdsSetupText", realmName, steps?join(msg("kdsAnd")))></#if>
<#return msg("kdsSetupPlainText", realmName)>
</#function>

<#function linkText>
<#local who = (identityProviderContext.username)!"">
<#if who?has_content><#return msg("kdsLinkText", identityProviderDisplayName, realmName, who)></#if>
<#return msg("kdsLinkPlainText", identityProviderDisplayName, realmName)>
</#function>
