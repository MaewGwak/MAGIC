# Horizon Launcher Core (MAGIC proof app)

A small Minecraft-style launcher core written in MAGIC. It proves file I/O,
arg parsing, string building, directory/file checks and process execution in a
real application rather than a microbenchmark.

```sh
magicc build apps/horizon/horizon.magic --backend c \
  --cc "gcc -O3 -march=native -lm" -o horizon

./horizon init /tmp/instance
# edit /tmp/instance/{java,memory,main,classpath}, then:
./horizon launch /tmp/instance
```

`init` creates the instance directory and four plain-text config files with
defaults (`java`, `2G`, `net.minecraft.client.main.Main`, empty classpath).
`launch` reads them, builds `java -Xmx<memory> -cp <classpath> <main>` and runs
it via `proc_run` (C `system`). A fake `/bin/echo` java is enough to test the
command path offline.

Current scope is the launcher **core**. It does not yet download Mojang
metadata/assets, verify hashes, manage multiple instances, or parse JSON; those
are the next application milestones. New builtins added for it:
`str_cat`, `str_from_bytes`, `proc_run`, `dir_mkdir`, `path_exists`
(C backend; explicit diagnostics on the native IR path).
