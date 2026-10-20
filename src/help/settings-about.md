---
title: Settings › About
kind: screen
screens: [settings/about]
order: 37
summary: The app's version and updates, its data folder, its logs and its licence.
---
About shows which Brainstead you're running and where it keeps its own files. **Website** opens Brainstead's website in your browser. Brainstead › About Brainstead shows the version and the website's address, which you can click, and Help › Brainstead Website opens it too.

## Find the version and app data folder
**Version** gives the version and build. **App data** is Brainstead's own folder, outside the vault: the index, logs, summary runs, unsaved Ask chats (and a saved chat's new turns while the vault is read-only), and `summaries/automated.json` if you have one. An assistant can tell you both.

## Updates
Brainstead checks for a new version 20 seconds after it opens and then once a day. When there is one it asks in its window the next time the window has the focus, with what's new in it: **Install and Restart**, **Later** (asked again at the next day's check) or **Skip This Version**. Brainstead › Check for Updates… checks now and says so when you're up to date. Installing shows the download's progress, then restarts. An update waits while something is running, such as Ask answering, an ingest or a summary; restarting asks about unsaved edits first, as quitting does. On Windows Brainstead doesn't check yet, because releases carry only the Mac app: install a new version with its installer.

## Find the logs
**The app's logs** are one file a day in the app data folder. **Keep** them for 7, 14, 30 or 90 days (14 by default); older files are deleted. **Show in Finder** (**Show in Explorer** on Windows) opens the folder, which helps when reporting a problem.

## Licence
Brainstead is free software under the GNU General Public License, version 3 or later. You may use, study, share and change it under that licence; the full text is in the LICENSE file that comes with the source.
