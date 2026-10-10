<#ftl output_format="plainText">
${msg("passwordResetSubject")}

${msg("kdsResetText", realmName)}

${link}

${msg("kdsResetNote", linkExpirationFormatter(linkExpiration))}
