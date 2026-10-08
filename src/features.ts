// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// What this platform can do (Quick Look, spelling, Export PDF…), read by the screens that offer it.
// Loaded once at startup (src/main.tsx); until then every feature counts as available.

import { api, PlatformFeatures } from "./api";
import { platform, setPlatform } from "./platform";
import { Store, useStore } from "./store";

export const features = new Store<PlatformFeatures>({
  platform: platform(),
  quickLook: true,
  spelling: true,
  grammar: true,
  readAloud: true,
  exportPdf: true,
  richCopy: true,
  fullDiskAccess: true,
  openAtLogin: true,
  imageText: true,
});

/** Asks the backend which features exist and adopts the platform it reports. */
export async function loadFeatures() {
  const f = await api.platformFeatures();
  features.set(f);
  if (f.platform !== platform()) setPlatform(f.platform);
}

export const useFeatures = () => useStore(features);
