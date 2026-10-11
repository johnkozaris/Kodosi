<#ftl output_format="plainText">
<#import "../sentences.ftl" as say>
${msg("executeActionsSubject")}

${say.setupText()}

${link}

${msg("kdsSetupNote", linkExpirationFormatter(linkExpiration))}
