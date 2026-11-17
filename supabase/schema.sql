-- Run this in Supabase SQL Editor (https://supabase.com/dashboard -> SQL Editor)

create table if not exists public.community_mods (
  id text primary key,
  name text not null,
  version text not null,
  author text not null default '',
  summary text not null default '',
  description text not null default '',
  instructions text not null default '',
  download_url text not null,
  icon_url text,
  previews jsonb not null default '[]'::jsonb,
  sha256 text not null,
  virustotal_url text,
  vt_malicious integer not null default 0,
  vt_total integer not null default 0,
  changelog jsonb not null default '[]'::jsonb,
  created_at timestamptz not null default now()
);

create table if not exists public.community_mod_secrets (
  mod_id text primary key references public.community_mods(id) on delete cascade,
  update_key_hash text not null
);

alter table public.community_mod_secrets enable row level security;

-- Allow anyone to read community_mods (inserts happen only via the Edge Function using service_role)
alter table public.community_mods enable row level security;

create policy "Public read community_mods"
  on public.community_mods
  for select
  to anon
  using (true);

-- Create public storage bucket for community mod files
insert into storage.buckets (id, name, public)
values ('community-mods', 'community-mods', true)
on conflict (id) do nothing;

create policy "Public read community-mods bucket"
  on storage.objects
  for select
  to public
  using (bucket_id = 'community-mods');
