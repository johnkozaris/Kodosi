import { i18nBuilder } from "keycloakify/login/i18n/noJsx";
import { useEffect, useState } from "react";

import type { ThemeName } from "../kc.gen";
import { plain } from "../parts/text";
import type { KcContext } from "./KcContext";

// The words of the pages. Keycloakify reads this call when it builds the theme and writes the
// words into Keycloak's message files, so they are in the call itself. A key without the "kds"
// start is one of Keycloak's own messages, in Kodosi's words. The apps have English only, so the
// pages have English only: in a different language, Keycloak's own words show.
const { getI18n, ofTypeI18n } = i18nBuilder
  .withThemeName<ThemeName>()
  .withCustomTranslations({
    en: {
      kdsSignIn: "Sign in",
      kdsContinue: "Continue",
      kdsCancel: "Cancel",
      kdsNotNow: "Not now",
      kdsTryAgain: "Try again",
      kdsBack: "Back",
      kdsBackToSignIn: "Back to sign in",
      kdsBackTo: "Back to {0}",
      kdsLanguage: "Language",
      kdsNotYou: "Sign in as a different person",
      kdsAnotherWay: "Try another way",

      kdsEmail: "Email",
      kdsUsername: "Username",
      kdsUsernameOrEmail: "Username or email",
      kdsPassword: "Password",
      kdsShowPassword: "Show the password",
      kdsHidePassword: "Hide the password",
      kdsCapsLock: "Caps Lock",
      kdsKeepSignedIn: "Keep me signed in",
      kdsForgot: "Forgot password?",
      kdsPasskeySignIn: "Sign in with a passkey",
      kdsContinueWith: "Continue with {0}",
      kdsLastUsed: "You used this the last time",
      kdsNewHere: "New here?",
      kdsMakeAccount: "Make an account",

      // The code that the Kodosi app shows while it waits for the browser.
      kdsCodeFromApp: "The code from Kodosi",
      kdsDeviceTitle: "Enter the code from Kodosi",
      kdsDeviceLead: "Kodosi shows it on your computer.",
      kdsCheckTitle: "Check the code",
      kdsCheckLead: "{0} on your computer shows the same code.",
      kdsConnectTitle: "Connect {0}?",
      // Without the code, the page cannot show which computer asks. A person who did not start a
      // sign-in stops here.
      kdsConnectLead: "Only if you just started a sign-in in {0}.",
      kdsConnect: "Connect",
      kdsNotSame: "Not the same",
      kdsGets: "{0} gets:",
      kdsGrantTitle: "{0} wants access",
      kdsGrantAllow: "Allow",
      kdsDoneTitle: "You are in",
      kdsDoneLead: "Go back to Kodosi.",
      kdsTagline: "terminals, agents and friends. together.",
      kdsDeniedTitle: "Not connected",
      kdsDeniedLead: "Nothing changed. To try again, start the sign-in in Kodosi.",

      kdsHandleTitle: "Pick your username",
      kdsHandleLead: "Friends find you by it. You cannot change it later.",
      kdsHandleSample: "yourname",
      kdsDetailsTitle: "Add your details",
      kdsCheckDetailsTitle: "Check your details",
      kdsPasswordTitle: "Choose a password",
      kdsFirstName: "First name",
      kdsLastName: "Last name",
      kdsNext: "Next",
      kdsMake: "Make my account",
      kdsSave: "Save",
      kdsHaveAccount: "I have an account",
      kdsAcceptTerms: "I accept the terms",
      kdsReadTerms: "Read them",
      kdsOptional: "Optional",
      // The check that a person, not a program, sends the form (parts/captcha.tsx).
      kdsNotRobot: "Show that you are a person",
      kdsCheckFailed: "The check did not pass.",
      kdsCheckAgain: "Try again",
      kdsPersonCheckLead: "A short check, then you go on.",
      kdsJoinTitle: "Make an account",
      kdsFill: "Fill in this row.",
      kdsBadEmail: "That email is not right.",
      kdsBadLength: "Use {0} to {1} characters.",
      kdsTooShort: "Use {0} or more characters.",
      kdsBadCharacters: "That has a character that you cannot use here.",
      kdsPolicyLength: "{0} or more characters",
      kdsPolicyDigits: "A digit",
      kdsPolicyLower: "A small letter",
      kdsPolicyUpper: "A capital letter",
      kdsPolicySpecial: "A special character",
      kdsPolicyMax: "{0} characters at most",
      kdsPolicyNotUsername: "Not your username",
      kdsPolicyNotEmail: "Not your email",

      kdsForgotTitle: "Forgot your password?",
      kdsForgotLead: "We send you a link. With it, you choose a new password.",
      kdsSendLink: "Send the link",

      kdsNewPasswordTitle: "Choose a password",
      kdsNewPassword: "New password",
      kdsSavePassword: "Save the password",
      kdsSignOutOthers: "Sign out everywhere else",

      kdsInboxTitle: "Check your inbox",
      kdsInboxLead: "We sent a link to {0}. Open it to go on.",
      kdsInboxLeadPlain: "We sent you a link. Open it to go on.",
      kdsSendAgain: "Send it again",
      kdsSentAgain: "Sent again",
      kdsSendAgainIn: "In {0} seconds",
      kdsOpenMail: "Open {0}",
      kdsSend: "Send the link",
      kdsConfirmEmailTitle: "Confirm your email",
      kdsConfirmEmailLead: "We send a link to {0}.",

      kdsCodeTitle: "Enter your code",
      kdsCodeLead: "The digits that your authenticator app shows now.",
      kdsCode: "Code",

      kdsAuthenticatorTitle: "Set up your authenticator",
      kdsAuthenticatorLead: "An app on your phone gives a new code for each sign-in.",
      kdsScan: "Scan this with your authenticator app",
      kdsCannotScan: "Can't scan it?",
      kdsTypeKey: "Type this key into your authenticator app",
      kdsScanInstead: "Scan a picture",
      kdsThenCode: "Then type the code that the app shows",
      kdsPhoneName: "A name for this phone",
      kdsFinish: "Finish",

      kdsPasskeyTitle: "Use your passkey",
      kdsPasskeyLead: "Your computer asks for your fingerprint, your face or your PIN.",
      kdsPasskeyUse: "Use the passkey",
      kdsPasskeyWaiting: "Waiting for your computer",
      kdsPasskeyNewTitle: "Make a passkey",
      kdsPasskeyNewLead:
        "The next time, you sign in with your fingerprint, your face or your PIN. No password.",
      kdsPasskeyCreate: "Make the passkey",
      kdsPasskeyName: "A name for this passkey",
      kdsPasskeyFailedTitle: "The passkey did not work",
      // Keycloak's sentences about a passkey, in the words of Kodosi. A person who closes the
      // passkey box of the browser gets the first.
      "webauthn-error-api-get": "The passkey did not answer. Try again, or use your password.",
      "webauthn-error-auth-verification": "The passkey did not work. Try again.",
      "webauthn-error-different-user": "That passkey is for a different account.",
      "webauthn-error-user-not-found": "That passkey is not for an account here.",
      "webauthn-error-registration": "The passkey was not made. Try again.",
      "webauthn-error-register-verification": "The passkey was not made. Try again.",
      "webauthn-error-title": "The passkey did not work",

      kdsRecoveryTitle: "Keep these codes",
      kdsRecoveryLead: "Each code signs you in one time, when you do not have your phone.",
      kdsCopy: "Copy",
      kdsCopied: "Copied",
      kdsDownload: "Download",
      kdsSavedCodes: "I saved these codes",
      kdsDone: "Done",
      kdsRecoveryCodeTitle: "Enter a recovery code",
      kdsRecoveryCodeLead: "Use code number {0} of your list.",
      kdsRecoveryCode: "Recovery code",

      kdsWaysTitle: "Choose a way to sign in",
      kdsWayPassword: "Password",
      kdsWayPasskey: "Passkey",
      kdsWayAuthenticator: "Code from your authenticator app",
      kdsWayRecovery: "Recovery code",

      kdsNewEmailTitle: "Change your email",

      kdsOneMoreStep: "One more step",
      kdsNoteTitle: "About your account",
      kdsErrorTitle: "That did not work",
      kdsExpiredTitle: "This page waited too long",
      kdsStartAgain: "Start again",
      kdsContinueWhere: "Go on where I was",

      kdsSignOutTitle: "Sign out?",
      kdsSignOut: "Sign out",

      kdsTermsTitle: "Terms",
      kdsAccept: "Accept",
      kdsDecline: "Decline",

      kdsRemoveTitle: "Remove “{0}”?",
      kdsRemove: "Remove",

      // The deletion of an account. The Kodosi server removes the person's data when Keycloak
      // deletes the account; these words say what goes and what stays.
      kdsDeleteTitle: "Delete your Kodosi account?",
      kdsDeleteLead: "This cannot be undone.",
      kdsDeleteGoes: "Your sign-in, devices, friends and sharing are deleted.",
      kdsDeleteRooms: "Rooms that you own close for everyone in them.",
      kdsDeleteWords: "What you wrote in other people's rooms stays there, from “deleted account”.",
      kdsDeleteTerminals: "Terminals on your computers keep running.",
      kdsDelete: "Delete account",
      kdsKeepAccount: "Keep my account",
      kdsDeletedTitle: "Your account is deleted",
      kdsDeletedLead: "You can close this tab.",

      kdsLinkTitle: "Add {0} to your account?",
      kdsLinkLead: "An account has this email already.",
      kdsLinkLeadOf: "An account has the email {0} already.",
      kdsLinkAddTo: "Add {0} to @{1}",
      kdsLinkOtherName: "Pick a different username",
      kdsHandleTaken: "is taken",
      kdsHandleTakenTab: "That username is taken",
      kdsHandleTakenLead: "If it is your account, add {0} to it.",
      kdsLinkAdd: "Add to my account",
      kdsLinkReview: "Check my details",
      kdsLinkEmailLead: "We sent a link to the email of your account. Open it to add {0}.",
      kdsLinkVerified: "I opened the link",

      // Keycloak's own messages, in the words of Kodosi. A wrong name and a wrong password get
      // the same sentence, and so does each forgotten password.
      invalidUserMessage: "That name or password is not right.",
      invalidPasswordMessage: "That name or password is not right.",
      invalidUsernameMessage: "That name is not right.",
      invalidUsernameOrEmailMessage: "That name is not right.",
      invalidTotpMessage: "That code is not right. Type the code that the app shows now.",
      expiredCodeMessage: "The sign-in waited too long. Please sign in again.",
      loginTimeout: "The sign-in waited too long. It starts again here.",
      emailSentMessage: "If the account exists, a link is on its way to you.",
      notMatchPasswordMessage: "The two passwords are not the same.",
      usernameExistsMessage: "That username is taken. Pick a different one.",
      // The first sign-in with a service found an account with the same username or email. The
      // page reads the detail and its value from this sentence (pages/choices.tsx).
      federatedIdentityConfirmLinkMessage: "An account with the {0} {1} exists already.",
      "error-user-attribute-required": "Fill in each marked row.",
      termsAcceptanceRequired: "Accept the terms to go on.",
      successLogout: "You are signed out",
      userDeletedSuccessfully: "Your account is deleted.",
      accountTemporarilyDisabledMessage:
        "This account is locked for a few minutes. Try again later.",
      accountTemporarilyDisabledMessageTotp:
        "This account is locked for a few minutes. Try again later.",
      // What a check against programs says when it refuses a form. The row of the check shows it.
      recaptchaFailed: "The check did not pass.",
      recaptchaNotConfigured: "The check did not load. Try again in a moment.",
      turnstileVerificationFailed: "The check did not pass.",
      turnstileMissingToken: "The check is not done yet. Wait a moment, then send again.",
      turnstileVerificationError: "The check did not answer. Try again in a moment.",
      turnstileIpBlocked: "This network cannot send this form.",
      oauth2DeviceInvalidUserCodeMessage: "That code is not right. Check the code in Kodosi.",
      oauth2DeviceExpiredUserCodeMessage:
        "That code is too old. Start the sign-in again in Kodosi.",
      oauth2DeviceVerificationCompleteHeader: "You are in",
      oauth2DeviceVerificationCompleteMessage: "Go back to Kodosi.",
      oauth2DeviceVerificationFailedHeader: "The sign-in did not work",
      oauth2DeviceVerificationFailedMessage: "Start the sign-in again in Kodosi.",
      oauth2DeviceConsentDeniedMessage:
        "Nothing changed. To try again, start the sign-in in Kodosi.",
      profileScopeConsentText: "Your name and your username",
      emailScopeConsentText: "Your email",
      offlineAccessScopeConsentText: "A sign-in that stays on your computer",

      // What a person in trouble reads: what happened, and what to do next.
      accountUpdatedMessage: "Your account is up to date",
      accountPasswordUpdatedMessage: "Your password is new",
      emailVerifiedMessage: "Your email is confirmed",
      emailVerifiedAlreadyMessage: "Your email is confirmed already",
      verifyEmailMessage: "Confirm your email to use your account.",
      confirmEmailAddressVerification: "Confirm that {0} is your email.",
      confirmEmailAddressVerificationHeader: "Confirm your email",
      emailVerifiedMessageHeader: "Your email is confirmed",
      emailVerifySendCooldown: "Wait {0} seconds, then send it again.",
      confirmExecutionOfActions: "Do these steps for your account",
      staleEmailVerificationLink: "That link is too old. Your email can be confirmed already.",
      alreadyLoggedIn: "You are signed in already",
      expiredActionMessage: "That link is too old. Sign in to go on.",
      expiredActionTokenNoSessionMessage: "That link is too old.",
      expiredActionTokenSessionExistsMessage: "That link is too old. Please start again.",
      staleCodeMessage: "This page is too old. Start the sign-in again.",
      cookieNotFoundMessage:
        "Your browser did not keep this sign-in. Allow cookies for this page, then start again.",
      differentUserAuthenticated: "This browser is signed in as {0}. Sign out first.",
      identityProviderUnexpectedErrorMessage:
        "The sign-in with that service did not work. Try again, or use a different way.",
      identityProviderAuthenticationFailedMessage:
        "The sign-in with that service did not work. Try again, or use a different way.",
      identityProviderLinkSuccess:
        "Your email is confirmed. Go back to the first browser window and go on there.",
      pageNotFound: "This page is not here",
      internalServerError: "Something went wrong on our side. Try again in a moment.",

      missingUsernameMessage: "Type your username.",
      missingPasswordMessage: "Type your password.",
      missingEmailMessage: "Type your email.",
      invalidEmailMessage: "That email is not right.",
      // Keycloak refuses an email that has an account. The words give the two ways on.
      emailExistsMessage: "Use a different email, or sign in.",
      "error-invalid-email": "That email is not right.",
      "error-invalid-length": "Use {1} to {2} characters.",
      "error-invalid-length-too-short": "Use {1} or more characters.",
      "error-invalid-length-too-long": "Use {2} characters or fewer.",
      "error-pattern-no-match": "That has a character that you cannot use here.",
      "error-username-invalid-character": "That has a character that you cannot use here.",
      "error-person-name-invalid-character": "That has a character that you cannot use here.",
      invalidPasswordMinLengthMessage: "The password must have {0} or more characters.",
      invalidPasswordMinDigitsMessage: "The password must have {0} or more digits.",
      invalidPasswordMinLowerCaseCharsMessage: "The password must have {0} or more small letters.",
      invalidPasswordMinUpperCaseCharsMessage:
        "The password must have {0} or more capital letters.",
      invalidPasswordMinSpecialCharsMessage:
        "The password must have {0} or more special characters.",
      invalidPasswordNotUsernameMessage: "The password must not be your username.",
      invalidPasswordNotContainsUsernameMessage: "The password must not have your username in it.",
      invalidPasswordNotEmailMessage: "The password must not be your email.",
      invalidPasswordHistoryMessage: "Choose a password that you did not use before.",
      invalidPasswordBlacklistedMessage:
        "That password is too easy to guess. Choose a different one.",
      invalidPasswordGenericMessage: "The password does not follow the rules.",

      "requiredAction.CONFIGURE_TOTP": "Set up your authenticator",
      "requiredAction.TERMS_AND_CONDITIONS": "Accept the terms",
      "requiredAction.UPDATE_PASSWORD": "Choose a new password",
      "requiredAction.UPDATE_PROFILE": "Check your details",
      "requiredAction.VERIFY_EMAIL": "Confirm your email",
      "requiredAction.UPDATE_EMAIL": "Change your email",
      "requiredAction.webauthn-register": "Add a security key",
      "requiredAction.webauthn-register-passwordless": "Make a passkey",
      "requiredAction.delete_account": "Delete your account",
      "requiredAction.CONFIGURE_RECOVERY_AUTHN_CODES": "Make recovery codes",
    },
  })
  .build();

export type I18n = typeof ofTypeI18n;

/**
 * The words of the page. The pages' own words are here from the start. Keycloak's words for a
 * different language (the name of a field that a realm added) come a moment later.
 */
export function useI18n({ kcContext }: { kcContext: KcContext }): { i18n: I18n } {
  const { i18n, prI18n_currentLanguage: later } = getI18n({ kcContext });
  const [words, setWords] = useState(i18n);
  useEffect(() => {
    let here = true;
    void later?.then((loaded) => {
      if (here) setWords(loaded);
    });
    return () => {
      here = false;
    };
  }, [later]);
  return { i18n: words };
}

/** Keycloak gives a sentence as its key, or as the words of that key. */
export function isMessage(i18n: I18n, text: string | undefined, key: string): boolean {
  if (!text) return false;
  if (text === key) return true;
  const words = i18n.msgStr(key as Parameters<I18n["msgStr"]>[0]);
  return words !== key && plain(text) === plain(words);
}
