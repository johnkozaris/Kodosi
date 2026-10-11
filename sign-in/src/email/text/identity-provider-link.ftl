<#ftl output_format="plainText">
<#import "../sentences.ftl" as say>
${msg("kdsLinkTitle")}

${say.linkText()}

${link}

${msg("kdsLinkNote", linkExpirationFormatter(linkExpiration))}
