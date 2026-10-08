// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// What the editor needs from the app: completions from the index, image URLs through the asset
// protocol, opening links, and saving pasted or dropped images into the vault's images/ folder.

import { convertFileSrc } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";
import { api } from "../api";
import { imageView } from "../ImageViewer";
import { suggest } from "../Capture";
import { imageSource, safeDecode } from "../md/Markdown";
import { localToday } from "../md/taskQuery";
import { openDoc } from "../nav";
import { reportEditError } from "../taskModel";
import type { EditorHooks } from "./Editor";

const join = (root: string, rel: string) => `${root.replace(/[\\/]$/, "")}/${rel.replace(/^\/+/, "")}`;

/** The embed for an image saved at a vault path, as markdown vault editors write it. */
export const imageEmbed = (rel: string) => `![[${rel}]]`;

/** "Pasted image 20261002145501.png", as other editors name them. */
export function pastedName(type: string, d = new Date(), n = 0): string {
  const p = (x: number) => String(x).padStart(2, "0");
  const ext = (type.split("/")[1] ?? "png").replace("jpeg", "jpg").replace(/\+.*$/, "");
  const stamp = `${d.getFullYear()}${p(d.getMonth() + 1)}${p(d.getDate())}${p(d.getHours())}${p(d.getMinutes())}${p(d.getSeconds())}`;
  return `Pasted image ${stamp}${n ? `-${n}` : ""}.${ext}`;
}

async function toBase64(f: Blob): Promise<string> {
  const buf = new Uint8Array(await f.arrayBuffer());
  let s = "";
  for (let i = 0; i < buf.length; i += 0x8000) s += String.fromCharCode(...buf.subarray(i, i + 0x8000));
  return btoa(s);
}

export function appHooks(path: string, root: string, pickDate?: EditorHooks["pickDate"]): EditorHooks {
  const dir = path.includes("/") ? path.slice(0, path.lastIndexOf("/")) : "";
  return {
    suggest,
    pickDate,
    today: () => localToday(),
    async imageUrl(src) {
      const w = imageSource(src);
      if ("url" in w) return w.url;
      if ("name" in w) {
        // `![[images/pic.png]]` is a path; `![[pic.png]]` a name to look up.
        if (w.name.includes("/")) return convertFileSrc(join(root, w.name));
        const p = await api.assetFind(w.name).catch(() => null);
        return p ? convertFileSrc(join(root, p)) : null;
      }
      return convertFileSrc(join(root, dir && !w.path.includes("/") ? `${dir}/${w.path}` : w.path));
    },
    openImage(url, alt) {
      imageView.set({ url, alt });
    },
    openLink(target, wiki) {
      if (!wiki) {
        if (/^(https?:|mailto:)/.test(target)) void openUrl(target);
        else void api.linksResolve([safeDecode(target.split("#")[0])]).then(([p]) => p && openDoc(p));
        return;
      }
      const hash = target.indexOf("#");
      const note = (hash < 0 ? target : target.slice(0, hash)).trim();
      const frag = hash < 0 ? undefined : target.slice(hash + 1).trim();
      const go = (p: string | null) => p && openDoc(p, frag ? { anchor: frag } : {});
      if (!note) go(path);
      else void api.linksResolve([note]).then(([p]) => go(p));
    },
    async pasteImages(files) {
      const out: string[] = [];
      for (const [i, f] of files.entries()) {
        try {
          const rel = await api.imageSave(
            f.name && !/^image\.\w+$/.test(f.name) ? f.name : pastedName(f.type, new Date(), i),
            await toBase64(f),
          );
          out.push(imageEmbed(rel));
        } catch (e) {
          reportEditError(e);
        }
      }
      return out.join("\n");
    },
  };
}
