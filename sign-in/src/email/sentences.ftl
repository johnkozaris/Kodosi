<#--
  The sentences that need more than one line of words. The html form and the text form of an
  e-mail take them from here, so the two forms say the same.
-->

<#-- The time of an event: the date as year, month and day, the hour and the zone. Each language
     reads it the same way. -->
<#function moment>
<#return event.date?string("yyyy-MM-dd HH:mm zzz")>
</#function>

<#-- The words here for a name that Keycloak gives: a step, or a kind of sign-in. A name with no
     words gives nothing. Keycloak reads a name that has no words as a message format, so a name
     with other characters is not looked for. -->
<#function wordsOf prefix name>
<#if !name?matches("[A-Za-z0-9_.-]+")><#return ""></#if>
<#local words = msg(prefix + name)>
<#if words == prefix + name><#return ""></#if>
<#return words>
</#function>

<#-- A note about the account: what took place, when, and from which address. -->
<#function eventText key>
<#return msg(key, moment(), event.ipAddress)>
</#function>

<#-- A note about a way of sign-in. It names the kind when the kind has words here. -->
<#function wayText withKind plain>
<#local kind = wordsOf("kdsWay.", event.getDetail("credential_type")!"")>
<#if kind?has_content><#return msg(withKind, kind, moment(), event.ipAddress)></#if>
<#return msg(plain, moment(), event.ipAddress)>
</#function>

<#-- What the set-up e-mail asks for: the steps that have words here, each one time. A step with
     no words is not named: the link opens it. -->
<#function setupText>
<#local steps = []>
<#list requiredActions![] as step>
<#local words = wordsOf("requiredAction.", step)>
<#if words?has_content && !steps?seq_contains(words)><#local steps = steps + [words]></#if>
</#list>
<#if steps?size gt 0><#return msg("kdsSetupText", realmName, steps?join(msg("kdsAnd")))></#if>
<#return msg("kdsSetupPlainText", realmName)>
</#function>

<#-- What the link of two accounts does. A sign-in service can give no name for its account. -->
<#function linkText>
<#local who = (identityProviderContext.username)!"">
<#if who?has_content><#return msg("kdsLinkText", identityProviderDisplayName, realmName, who)></#if>
<#return msg("kdsLinkPlainText", identityProviderDisplayName, realmName)>
</#function>
