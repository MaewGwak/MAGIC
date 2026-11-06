# Horizon Launcher Core (MAGIC proof app)

A small Minecraft-style launcher core written in MAGIC. It proves file I/O,
arg parsing, string building, directory/file checks and process execution in a
real application rather than a microbenchmark.

```sh
magicc build apps/horizon/horizon.magic --backend c \
  --cc "gcc -O3 -march=native -lm" -o horizon

./horizon init /tmp/instance
./horizon show /tmp/instance
./horizon set /tmp/instance memory 4G
./horizon list /tmp/instance
./horizon import /tmp/instance version.json
./horizon assets /tmp/instance version.json
./horizon verify /tmp/instance hashes.json
# edit /tmp/instance/{java,memory,main,classpath}, then:
./horizon launch /tmp/instance
```

`init` creates the instance directory and plain-text config files with defaults
(`java`, `2G`, `net.minecraft.client.main.Main`, empty classpath, `1.20.1`,
empty `jar`, `jvmargs`, `gameargs`). `show` prints them, `set <dir> <key>
<value>` updates one, `list <dir>` prints directory entries (use the instances
root), `import <dir> <version.json>` extracts `id` and `mainClass` from a local
Mojang-style version manifest (small JSON key/string scanner), `doctor`
validates required config, and `launch` builds
`java -Xmx<memory> [jvmargs] -cp <classpath> <main> [gameargs]` (or `-jar
<jar>` when a jar is configured) and runs it via `proc_run` (C `system`). A fake `/bin/echo` java is enough to test the command path offline.

Current scope is the launcher **core**. It does not yet download Mojang
metadata/assets, verify hashes, manage multiple instances, or parse JSON; those
are the next application milestones. New builtins added for it:
`str_cat`, `str_from_bytes`, `proc_run`, `dir_mkdir`, `path_exists`
(C backend; explicit diagnostics on the native IR path).
