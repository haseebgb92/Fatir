# Fatir Companion — local MVP

Native Android companion for Fatir Desktop.

## Current scope

- manual LAN connection using Fatir Desktop IP + device token;
- remote chat through the same Fatir runtime;
- approve / deny pending Fatir actions;
- browse Companion-approved Linux roots;
- attach an existing Linux file to a Fatir chat;
- download a Linux file to Android using the Android document picker;
- upload an Android document to Fatir's Linux staging area;
- light Fatir-inspired UI with a floating menu and bottom composer.

This branch intentionally uses cleartext HTTP on a trusted local network while the transport is being proven. Do not expose port `32145` to the public internet. Encrypted pairing/transport is the next security milestone before any remote-access work.

## Build

Open `companion-android` in Android Studio, let Gradle sync, then run the `app` configuration on an Android phone on the same Wi-Fi as the Linux machine.
