// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

import type { ExtensionAPI } from "@earendil-works/pi-coding-agent";

const BANNER = String.raw`
  ___                 ___  _____   _____
 / _ \ _ __  ___ _ _ / __|/ _ \ \ / /   \
| (_) | '_ \/ -_) ' \\__ \ (_) \ V /| |) |
 \___/| .__/\___|_||_|___/\___/ \_/ |___/
      |_|`.split("\n").slice(1);

export default function (pi: ExtensionAPI) {
	pi.on("session_start", async (_event, ctx) => {
		if (ctx.mode !== "tui") return;
		ctx.ui.setHeader((_tui, theme) => ({
			render: () => ["", ...BANNER.map((line) => theme.fg("accent", line)), ""],
			invalidate() {},
		}));
		ctx.ui.setFooter((_tui, theme) => ({
			render: (width: number) => {
				const model = ctx.model?.id ?? "";
				return [" ".repeat(Math.max(0, width - model.length)) + theme.fg("dim", model)];
			},
			invalidate() {},
		}));
	});
}
