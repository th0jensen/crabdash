-- Crabdash for Rex: the dashboard's sections as Rex panes, from the command
-- palette of any Rex app connected to this machine, including Rex for iOS.
--
-- `crabdash-tui rex install` loads this from Rex's config (~/.config/rex/init.lua):
--
--   dofile("/path/to/plugins/crabdash/init.lua").setup({ bin = { "/path/to/crabdash-tui" } })
--
-- Panes run crabdash-tui on the machine whose session they open in. For a
-- session on another host, they run that host's crabdash-tui, installing it
-- with cargo first when the host has none. Rex carries the connection, so
-- crabdash-tui only ever reads the machine it runs on.

local M = {}

local SHELL = "com.superlogical.terminal.shell"

-- The desktop app's tabs, in its order.
local SECTIONS = {
  { name = "docker", title = "Docker", about = "containers: start, stop, restart, pause and logs" },
  { name = "disks", title = "Disks", about = "disks, partitions and mounts" },
  { name = "services", title = "Services", about = "system services: start, stop, restart and logs" },
  { name = "system", title = "System", about = "CPU, memory, network, disk activity and processes" },
}

-- Runs the host's crabdash-tui, installing it with cargo when it isn't there.
local BOOTSTRAP = [[
bin=$(command -v crabdash-tui 2>/dev/null || true)
[ -n "$bin" ] || bin="$HOME/.cargo/bin/crabdash-tui"
if [ ! -x "$bin" ]; then
  cargo=$(command -v cargo 2>/dev/null || echo "$HOME/.cargo/bin/cargo")
  if [ ! -x "$cargo" ]; then
    printf 'crabdash-tui isn'"'"'t on %s, and cargo isn'"'"'t either.\n' "$(uname -n)"
    printf 'Install Rust from https://rustup.rs, then: cargo install --locked crabdash-tui\n'
    printf 'Press enter to close.'; read -r _; exit 1
  fi
  printf 'Installing crabdash-tui on %s with cargo…\n' "$(uname -n)"
  if ! "$cargo" install --locked crabdash-tui; then
    printf '\ncrabdash-tui couldn'"'"'t be built here. The output above says why.\n'
    printf 'Press enter to close.'; read -r _; exit 1
  fi
fi
exec "$bin" "$@"
]]

function M.setup(opts)
  opts = opts or {}
  local bin = opts.bin or { "crabdash-tui" }
  if type(bin) == "string" then bin = { bin } end

  -- This machine's crabdash-tui, or the remote host's.
  local function command(ctx, section)
    if ctx.server == nil then
      local out = {}
      for _, part in ipairs(bin) do out[#out + 1] = part end
      out[#out + 1] = section
      return out
    end
    return { "sh", "-c", BOOTSTRAP, "crabdash-tui", section }
  end

  local function pane(ctx, section)
    return rex.layout.block{
      flavor = SHELL,
      label = section.title,
      options = { command = command(ctx, section.name) },
    }
  end

  local function fail(what, err)
    error("crabdash: couldn't open " .. what .. ": " .. tostring(err), 0)
  end

  rex.action{
    name = "crabdash_dashboard",
    title = "Crabdash: Open Dashboard",
    description = "Open a tab with a pane for each Crabdash section: Docker, Disks, Services and System.",
    category = "Crabdash",
    keywords = "crabdash dashboard homelab docker containers disks services system monitor",
    run = function(ctx)
      local p = function(index) return pane(ctx, SECTIONS[index]) end
      local _, err = rex.session.new_window{
        window_label = "Crabdash",
        focus = true,
        layout = rex.layout.horizontal(0.5,
          rex.layout.vertical(0.5, p(1), p(2)),
          rex.layout.vertical(0.5, p(3), p(4))),
      }
      if err ~= nil then fail("the dashboard", err) end
    end,
  }

  for _, section in ipairs(SECTIONS) do
    rex.action{
      name = "crabdash_" .. section.name,
      title = "Crabdash: " .. section.title,
      description = "Open Crabdash's " .. section.about .. " in a pane beside this one.",
      category = "Crabdash",
      keywords = "crabdash " .. section.name .. " " .. section.about,
      run = function(ctx)
        local layout = pane(ctx, section)
        local err
        if ctx.block_id == nil then
          _, err = rex.session.new_window{ window_label = section.title, layout = layout, focus = true }
        else
          _, err = rex.session.new_split{
            anchor_id = ctx.block_id,
            direction = "horizontal",
            side = "after",
            focus = true,
            layout = layout,
          }
        end
        if err ~= nil then fail(section.title, err) end
      end,
    }
  end

  for name, key in pairs(opts.keys or {}) do
    rex.bind(key, "crabdash_" .. name)
  end
end

return M
