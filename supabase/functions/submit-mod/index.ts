import { createClient } from "https://esm.sh/@supabase/supabase-js@2";

const corsHeaders = {
  "Access-Control-Allow-Origin": "*",
  "Access-Control-Allow-Headers": "authorization, x-client-info, apikey, content-type",
  "Access-Control-Allow-Methods": "POST, OPTIONS",
};

function json(data: unknown, status = 200) {
  return new Response(JSON.stringify(data), {
    status,
    headers: { ...corsHeaders, "Content-Type": "application/json" },
  });
}

async function sha256Hex(bytes: Uint8Array): Promise<string> {
  const hash = await crypto.subtle.digest("SHA-256", bytes);
  return Array.from(new Uint8Array(hash))
    .map((b) => b.toString(16).padStart(2, "0"))
    .join("");
}

function slugify(input: string): string {
  return input
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "")
    .slice(0, 48) || "mod";
}

const PENDING_MSG =
  "VirusTotal is still scanning this file. Please try submitting the same file again in a minute or two.";

async function scanWithVirusTotal(bytes: Uint8Array, filename: string, sha256: string, vtKey: string) {
  const res = await fetch(`https://www.virustotal.com/api/v3/files/${sha256}`, {
    headers: { "x-apikey": vtKey },
  });

  if (res.status === 404) {
    const form = new FormData();
    form.append("file", new Blob([bytes]), filename);
    const up = await fetch("https://www.virustotal.com/api/v3/files", {
      method: "POST",
      headers: { "x-apikey": vtKey },
      body: form,
    });
    if (!up.ok) {
      throw new Error(`VirusTotal upload failed (${up.status})`);
    }
    const upJson = await up.json();
    const analysisId = upJson?.data?.id;

    if (analysisId) {
      for (let i = 0; i < 5; i++) {
        await new Promise((r) => setTimeout(r, 3000));
        const check = await fetch(`https://www.virustotal.com/api/v3/analyses/${analysisId}`, {
          headers: { "x-apikey": vtKey },
        });
        if (check.ok) {
          const cj = await check.json();
          if (cj?.data?.attributes?.status === "completed") {
            const stats = cj?.data?.attributes?.stats ?? {};
            const malicious = Number(stats.malicious ?? 0) + Number(stats.suspicious ?? 0);
            const total = Object.values(stats).reduce((a: number, b) => a + Number(b || 0), 0);
            return {
              malicious,
              total,
              pending: total === 0,
              url: `https://www.virustotal.com/gui/file/${sha256}`,
            };
          }
        }
      }
    }
    return {
      malicious: 0,
      total: 0,
      pending: true,
      url: `https://www.virustotal.com/gui/file/${sha256}`,
    };
  }

  if (!res.ok) {
    throw new Error(`VirusTotal lookup failed (${res.status})`);
  }

  const info = await res.json();
  const stats = info?.data?.attributes?.last_analysis_stats ?? {};
  const malicious = Number(stats.malicious ?? 0) + Number(stats.suspicious ?? 0);
  const total = Object.values(stats).reduce((a: number, b) => a + Number(b || 0), 0);

  return {
    malicious,
    total,
    pending: total === 0,
    url: `https://www.virustotal.com/gui/file/${sha256}`,
  };
}

Deno.serve(async (req) => {
  if (req.method === "OPTIONS") {
    return new Response("ok", { headers: corsHeaders });
  }
  if (req.method !== "POST") {
    return json({ error: "Method not allowed" }, 405);
  }

  try {
    const supabaseUrl = Deno.env.get("SUPABASE_URL")!;
    const serviceKey = Deno.env.get("SUPABASE_SERVICE_ROLE_KEY")!;
    const vtKey = Deno.env.get("VIRUSTOTAL_API_KEY");
    if (!vtKey) {
      return json({ error: "VIRUSTOTAL_API_KEY secret is not configured" }, 500);
    }

    const authHeader = req.headers.get("authorization") || "";
    const jwt = authHeader.replace(/^Bearer\s+/i, "").trim();
    if (!jwt) {
      return json({ error: "Please sign in first." }, 401);
    }

    const supabase = createClient(supabaseUrl, serviceKey);
    const { data: userData, error: authErr } = await supabase.auth.getUser(jwt);
    if (authErr || !userData?.user) {
      return json({ error: "Session expired or invalid. Please sign in again." }, 401);
    }
    const user = userData.user;

    const form = await req.formData();
    const action = String(form.get("action") || "create").trim();
    const modIdInput = String(form.get("mod_id") || "").trim();
    const notes = String(form.get("notes") || "").trim();
    const name = String(form.get("name") || "").trim();
    const version = String(form.get("version") || "1.0.0").trim();
    const author = String(form.get("author") || "").trim();
    const summary = String(form.get("summary") || "").trim();
    const description = String(form.get("description") || "").trim();
    const instructions = String(form.get("instructions") || "").trim();
    const file = form.get("file");
    const icon = form.get("icon");

    if (action === "delete") {
      if (!modIdInput) return json({ error: "Missing mod_id." }, 400);
      const { data: existing } = await supabase
        .from("community_mods")
        .select("owner_id")
        .eq("id", modIdInput)
        .maybeSingle();
      if (!existing || existing.owner_id !== user.id) {
        return json({ error: "You do not own this mod." }, 403);
      }
      const { error: delErr } = await supabase.from("community_mods").delete().eq("id", modIdInput);
      if (delErr) return json({ error: delErr.message }, 500);
      return json({ ok: true });
    }

    if (action === "update") {
      if (!modIdInput || !(file instanceof File)) {
        return json({ error: "Mod ID and new .dll/.zip file are required." }, 400);
      }
      const { data: existing } = await supabase
        .from("community_mods")
        .select("owner_id, changelog")
        .eq("id", modIdInput)
        .maybeSingle();
      if (!existing || existing.owner_id !== user.id) {
        return json({ error: "You do not own this mod." }, 403);
      }

      const lowerName = file.name.toLowerCase();
      if (!lowerName.endsWith(".dll") && !lowerName.endsWith(".zip")) {
        return json({ error: "Only .dll or .zip mod files are allowed." }, 400);
      }
      if (file.size > 5 * 1024 * 1024) {
        return json({ error: "File size limit is 5 MB." }, 400);
      }

      const modBytes = new Uint8Array(await file.arrayBuffer());
      const sha256 = await sha256Hex(modBytes);
      const vt = await scanWithVirusTotal(modBytes, file.name, sha256, vtKey);
      if (vt.pending) {
        return json({ error: PENDING_MSG, virustotal_url: vt.url }, 202);
      }
      if (vt.malicious > 0) {
        return json(
          { error: `Update rejected: VirusTotal flagged this file (${vt.malicious}/${vt.total}).`, virustotal_url: vt.url },
          400,
        );
      }

      const safeFileName = file.name.replace(/[^a-zA-Z0-9._-]/g, "_");
      const modPath = `${modIdInput}/${safeFileName}`;
      const { error: upErr } = await supabase.storage
        .from("community-mods")
        .upload(modPath, modBytes, { upsert: true, contentType: "application/octet-stream" });
      if (upErr) {
        return json({ error: `Storage upload failed: ${upErr.message}` }, 500);
      }
      const downloadUrl = `${supabaseUrl}/storage/v1/object/public/community-mods/${modPath}`;

      const prevLog = Array.isArray(existing.changelog) ? existing.changelog : [];
      const today = new Date().toISOString().slice(0, 10);
      const nextLog = notes ? [{ version, date: today, notes }, ...prevLog] : prevLog;

      const { data: updatedRow, error: updErr } = await supabase
        .from("community_mods")
        .update({
          version,
          download_url: downloadUrl,
          sha256,
          virustotal_url: vt.url,
          vt_malicious: vt.malicious,
          vt_total: vt.total,
          changelog: nextLog,
        })
        .eq("id", modIdInput)
        .select()
        .single();
      if (updErr) {
        return json({ error: `Database error: ${updErr.message}` }, 500);
      }
      return json({ ok: true, mod: updatedRow, virustotal_url: vt.url });
    }

    // Create new mod
    if (!name || !author || !(file instanceof File)) {
      return json({ error: "Name, author, and mod file (.dll or .zip) are required." }, 400);
    }

    const lowerName = file.name.toLowerCase();
    if (!lowerName.endsWith(".dll") && !lowerName.endsWith(".zip")) {
      return json({ error: "Only .dll or .zip mod files are allowed." }, 400);
    }
    if (file.size > 5 * 1024 * 1024) {
      return json({ error: "File size limit is 5 MB." }, 400);
    }

    // Per-user rate limit: max 3 submissions per hour, plus 200 total cap.
    const hourAgo = new Date(Date.now() - 60 * 60 * 1000).toISOString();
    const { count: userRecent } = await supabase
      .from("community_mods")
      .select("id", { count: "exact", head: true })
      .eq("owner_id", user.id)
      .gte("created_at", hourAgo);
    if ((userRecent ?? 0) >= 3) {
      return json({ error: "Rate limit: max 3 mod submissions per hour per account." }, 429);
    }
    const { count: totalCount } = await supabase
      .from("community_mods")
      .select("id", { count: "exact", head: true });
    if ((totalCount ?? 0) >= 200) {
      return json({ error: "The community library is full for now." }, 429);
    }

    const modBytes = new Uint8Array(await file.arrayBuffer());
    const sha256 = await sha256Hex(modBytes);

    const vt = await scanWithVirusTotal(modBytes, file.name, sha256, vtKey);
    if (vt.pending) {
      return json({ error: PENDING_MSG, virustotal_url: vt.url }, 202);
    }
    if (vt.malicious > 0) {
      return json(
        {
          error: `Submission rejected: VirusTotal flagged this file (${vt.malicious}/${vt.total}).`,
          virustotal_url: vt.url,
        },
        400,
      );
    }

    const id = `community-${slugify(name)}-${sha256.slice(0, 6)}`;
    const { data: already } = await supabase
      .from("community_mods")
      .select("id")
      .eq("id", id)
      .maybeSingle();
    if (already) {
      return json({ error: "A mod with this name and file already exists." }, 409);
    }

    const safeFileName = file.name.replace(/[^a-zA-Z0-9._-]/g, "_");
    const modPath = `${id}/${safeFileName}`;
    const { error: upErr } = await supabase.storage
      .from("community-mods")
      .upload(modPath, modBytes, { upsert: true, contentType: "application/octet-stream" });
    if (upErr) {
      return json({ error: `Storage upload failed: ${upErr.message}` }, 500);
    }
    const downloadUrl = `${supabaseUrl}/storage/v1/object/public/community-mods/${modPath}`;

    let iconUrl: string | null = null;
    if (icon instanceof File && icon.size > 0 && icon.size <= 3 * 1024 * 1024) {
      const iconBytes = new Uint8Array(await icon.arrayBuffer());
      const safeIcon = icon.name.replace(/[^a-zA-Z0-9._-]/g, "_");
      const iconPath = `${id}/icon_${safeIcon}`;
      const { error: iconErr } = await supabase.storage
        .from("community-mods")
        .upload(iconPath, iconBytes, { upsert: true, contentType: icon.type || "image/png" });
      if (!iconErr) {
        iconUrl = `${supabaseUrl}/storage/v1/object/public/community-mods/${iconPath}`;
      }
    }

    const row = {
      id,
      name,
      version,
      author,
      summary,
      description,
      instructions,
      download_url: downloadUrl,
      icon_url: iconUrl,
      previews: [],
      sha256,
      virustotal_url: vt.url,
      vt_malicious: vt.malicious,
      vt_total: vt.total,
      changelog: [],
      owner_id: user.id,
    };

    const { error: dbErr } = await supabase.from("community_mods").insert(row);
    if (dbErr) {
      return json({ error: `Database error: ${dbErr.message}` }, 500);
    }

    return json({ ok: true, mod: row, virustotal_url: vt.url });
  } catch (err) {
    return json({ error: String(err) }, 500);
  }
});
