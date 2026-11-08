package io.github.pocketrecompiler;

import android.app.Activity;
import android.os.Bundle;
import android.content.Intent;
import android.net.Uri;
import android.widget.*;
import android.view.View;
import android.content.res.AssetManager;

import java.io.*;
import java.util.zip.*;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;

public class MainActivity extends Activity {
    private static final int PICK_ROM = 1;
    private static final int PICK_FONT = 2;
    private static final int SAVE_ZIP = 3;

    private Uri romUri, fontUri;
    private File packageDir;
    private TextView status;
    private Button buildButton, exportButton;
    private final ExecutorService worker = Executors.newSingleThreadExecutor();

    private void say(String text) {
        runOnUiThread(() -> status.append(text + "\n"));
    }

    private void pick(int code) {
        Intent i = new Intent(Intent.ACTION_OPEN_DOCUMENT);
        i.addCategory(Intent.CATEGORY_OPENABLE);
        i.setType("*/*");
        startActivityForResult(i, code);
    }

    @Override public void onCreate(Bundle state) {
        super.onCreate(state);

        LinearLayout layout = new LinearLayout(this);
        layout.setOrientation(LinearLayout.VERTICAL);
        layout.setPadding(28, 28, 28, 28);

        TextView heading = new TextView(this);
        heading.setText("Pocket-Native Forge");
        heading.setTextSize(26);
        layout.addView(heading);

        Button rom = new Button(this);
        rom.setText("1. Select decrypted .3ds ROM");
        rom.setOnClickListener(v -> pick(PICK_ROM));
        layout.addView(rom);

        Button font = new Button(this);
        font.setText("Optional: Select shared_font.bin");
        font.setOnClickListener(v -> pick(PICK_FONT));
        layout.addView(font);

        buildButton = new Button(this);
        buildButton.setText("2. Generate and compile on device");
        buildButton.setEnabled(false);
        buildButton.setOnClickListener(v -> startBuild());
        layout.addView(buildButton);

        exportButton = new Button(this);
        exportButton.setText("3. Export Pocket Native ZIP");
        exportButton.setEnabled(false);
        exportButton.setOnClickListener(v -> saveZip());
        layout.addView(exportButton);

        ScrollView scroll = new ScrollView(this);
        status = new TextView(this);
        status.setTextSize(13);
        status.setPadding(0, 20, 0, 20);
        scroll.addView(status);
        layout.addView(scroll, new LinearLayout.LayoutParams(
            -1, 0, 1
        ));
        setContentView(layout);

        worker.execute(() -> {
            try {
                installToolchain();
                installEngine();
                say("Compiler and build engine installed.");

                File savedPath = new File(getFilesDir(), "build-work/package.path");
                if (savedPath.isFile()) {
                    String path = new String(
                        java.nio.file.Files.readAllBytes(savedPath.toPath()), "UTF-8"
                    ).trim();
                    File restored = new File(path);
                    if (new File(restored, "game.3ds").isFile()
                            && new File(restored, "title.so").isFile()) {
                        packageDir = restored;
                        say("Restored previous build: " + restored.getName());
                        runOnUiThread(() -> exportButton.setEnabled(true));
                    }
                }

                runOnUiThread(() -> buildButton.setEnabled(romUri != null));
            } catch (Exception e) {
                say("Toolchain setup failed: " + e);
            }
        });
    }

    @Override protected void onActivityResult(int request, int result, Intent data) {
        super.onActivityResult(request, result, data);
        if (result != RESULT_OK || data == null) return;
        Uri uri = data.getData();
        if (uri == null) return;
        getContentResolver().takePersistableUriPermission(
            uri, Intent.FLAG_GRANT_READ_URI_PERMISSION
        );

        if (request == PICK_ROM) {
            romUri = uri;
            say("ROM selected.");
            buildButton.setEnabled(true);
        } else if (request == PICK_FONT) {
            fontUri = uri;
            say("Shared font selected.");
        } else if (request == SAVE_ZIP) {
            final Uri destination = uri;
            worker.execute(() -> writeZip(destination));
        }
    }

    private void installToolchain() throws Exception {
        File root = new File(getFilesDir(), "toolchain");
        File marker = new File(root, "installed.ok");
        if (marker.isFile()) return;

        say("Installing bundled Clang and LLD (first launch only)...");
        deleteTree(root);
        if (!root.mkdirs()) throw new Exception("Cannot create toolchain directory");
        copyAssets("toolchain", root);

        for (String name : new String[]{"clang", "lld"}) {
            File executable = new File(root, "bin/" + name);
            if (!executable.setExecutable(true, false))
                throw new Exception("Cannot enable execution for " + name);
        }
        if (!marker.createNewFile()) throw new Exception("Cannot mark toolchain installed");
    }

    private void installEngine() throws Exception {
        File bin = new File(getFilesDir(), "bin");
        if (!bin.exists() && !bin.mkdirs())
            throw new Exception("Cannot create engine directory");

        File engine = new File(bin, "pocket-recompiler-device");
        if (!engine.isFile())
            copyAssets("bin/pocket-recompiler-device", engine);

        if (!engine.setExecutable(true, false))
            throw new Exception("Cannot enable engine execution");
    }

    private void copyAssets(String path, File destination) throws Exception {
        String[] children = getAssets().list(path);
        if (children != null && children.length > 0) {
            if (!destination.exists() && !destination.mkdirs())
                throw new Exception("Cannot create " + destination);
            for (String child : children)
                copyAssets(path + "/" + child, new File(destination, child));
            return;
        }

        try (InputStream in = getAssets().open(path);
             OutputStream out = new BufferedOutputStream(new FileOutputStream(destination))) {
            byte[] buffer = new byte[131072];
            int n;
            while ((n = in.read(buffer)) != -1) out.write(buffer, 0, n);
        }
    }

    private File copyUri(Uri uri, File destination) throws Exception {
        try (InputStream in = getContentResolver().openInputStream(uri);
             OutputStream out = new BufferedOutputStream(new FileOutputStream(destination))) {
            if (in == null) throw new Exception("Cannot open selected file");
            byte[] buffer = new byte[1024 * 1024];
            int n;
            while ((n = in.read(buffer)) != -1) out.write(buffer, 0, n);
        }
        return destination;
    }

    private void startBuild() {
        buildButton.setEnabled(false);
        exportButton.setEnabled(false);
        status.setText("");
        worker.execute(() -> {
            try {
                File work = new File(getFilesDir(), "build-work");
                deleteTree(work);
                if (!work.mkdirs()) throw new Exception("Cannot create work directory");

                say("Copying ROM into app storage...");
                File rom = copyUri(romUri, new File(work, "input.3ds"));
                File font = null;
                if (fontUri != null) {
                    say("Copying shared font...");
                    font = copyUri(fontUri, new File(work, "shared_font.bin"));
                }

                File engine = new File(getFilesDir(), "bin/pocket-recompiler-device");
                File log = new File(work, "engine-output.log");
                File progress = new File(work, "progress.log");

                say("Starting on-device recompilation. Large games can take a while.");
                ProcessBuilder pb;
                if (font == null) {
                    pb = new ProcessBuilder(
                        "/system/bin/linker64", engine.getAbsolutePath(),
                        rom.getAbsolutePath(), work.getAbsolutePath(),
                        new File(getFilesDir(), "toolchain").getAbsolutePath()
                    );
                } else {
                    pb = new ProcessBuilder(
                        "/system/bin/linker64", engine.getAbsolutePath(),
                        rom.getAbsolutePath(), work.getAbsolutePath(),
                        new File(getFilesDir(), "toolchain").getAbsolutePath(),
                        font.getAbsolutePath()
                    );
                }
                pb.redirectErrorStream(true);
                pb.redirectOutput(log);
                Process process = pb.start();

                long offset = 0;
                while (process.isAlive()) {
                    if (progress.isFile()) {
                        try (RandomAccessFile f = new RandomAccessFile(progress, "r")) {
                            f.seek(offset);
                            String line;
                            while ((line = f.readLine()) != null) {
                                say(new String(line.getBytes("ISO-8859-1"), "UTF-8"));
                            }
                            offset = f.getFilePointer();
                        }
                    }
                    Thread.sleep(1000);
                }
                int exit = process.waitFor();

                if (exit != 0) {
                    String output = log.isFile()
                        ? new String(java.nio.file.Files.readAllBytes(log.toPath()), "UTF-8")
                        : "";
                    throw new Exception("Build failed (exit " + exit + "). " + output);
                }

                String packagePath = new String(
                    java.nio.file.Files.readAllBytes(new File(work, "package.path").toPath()),
                    "UTF-8"
                ).trim();
                packageDir = new File(packagePath);
                if (!new File(packageDir, "title.so").isFile())
                    throw new Exception("Build finished without title.so");

                say("Build complete. Ready to export.");
                runOnUiThread(() -> exportButton.setEnabled(true));
            } catch (Throwable e) {
                say("BUILD ERROR: " + e);
            } finally {
                runOnUiThread(() -> buildButton.setEnabled(romUri != null));
            }
        });
    }

    private void saveZip() {
        if (packageDir == null) return;
        Intent i = new Intent(Intent.ACTION_CREATE_DOCUMENT);
        i.addCategory(Intent.CATEGORY_OPENABLE);
        i.setType("application/zip");
        i.putExtra(Intent.EXTRA_TITLE, packageDir.getName() + ".zip");
        startActivityForResult(i, SAVE_ZIP);
    }

    private void writeZip(Uri destination) {
        File archiveDir = new File(getFilesDir(), "exports");
        File archive = new File(archiveDir, packageDir.getName() + ".zip");

        try {
            if (!archiveDir.exists() && !archiveDir.mkdirs())
                throw new Exception("Cannot create private export directory");

            File rom = new File(packageDir, "game.3ds");
            File library = new File(packageDir, "title.so");
            if (!rom.isFile() || !library.isFile())
                throw new Exception("Package is missing game.3ds or title.so");

            say("Creating ZIP in private storage...");
            if (archive.exists() && !archive.delete())
                throw new Exception("Cannot replace previous ZIP");

            try (ZipOutputStream zip = new ZipOutputStream(
                    new BufferedOutputStream(new FileOutputStream(archive)))) {
                zip.setLevel(java.util.zip.Deflater.NO_COMPRESSION);
                zipTree(packageDir, packageDir, zip);
            }

            say("Verifying ZIP...");
            try (ZipFile check = new ZipFile(archive)) {
                java.util.Enumeration<? extends ZipEntry> entries = check.entries();
                boolean hasRom = false;
                boolean hasLibrary = false;
                byte[] buffer = new byte[131072];

                while (entries.hasMoreElements()) {
                    ZipEntry entry = entries.nextElement();
                    if (entry.getName().equals("game.3ds")) hasRom = true;
                    if (entry.getName().equals("title.so")) hasLibrary = true;

                    try (InputStream in = check.getInputStream(entry)) {
                        while (in.read(buffer) != -1) { }
                    }
                }

                if (!hasRom || !hasLibrary)
                    throw new Exception("ZIP verification failed: required files missing");
            }

            say("ZIP verified. Copying to selected destination...");
            OutputStream raw = getContentResolver().openOutputStream(destination, "w");
            if (raw == null) throw new Exception("Cannot open ZIP destination");

            try (OutputStream out = new BufferedOutputStream(raw);
                 InputStream in = new BufferedInputStream(new FileInputStream(archive))) {
                byte[] buffer = new byte[131072];
                int n;
                while ((n = in.read(buffer)) != -1) out.write(buffer, 0, n);
            }

            say("ZIP export complete and verified: " + archive.getAbsolutePath());
        } catch (Exception e) {
            say("ZIP export failed: " + e);
        }
    }

    private void zipTree(File root, File current, ZipOutputStream zip) throws Exception {
        File[] files = current.listFiles();
        if (files == null) return;
        for (File file : files) {
            if (file.isDirectory()) {
                zipTree(root, file, zip);
            } else {
                String name = root.toPath().relativize(file.toPath()).toString()
                    .replace(File.separatorChar, '/');
                zip.putNextEntry(new ZipEntry(name));
                try (InputStream in = new BufferedInputStream(new FileInputStream(file))) {
                    byte[] buffer = new byte[131072];
                    int n;
                    while ((n = in.read(buffer)) != -1) zip.write(buffer, 0, n);
                }
                zip.closeEntry();
            }
        }
    }

    private void deleteTree(File file) {
        if (!file.exists()) return;
        if (file.isDirectory()) {
            File[] children = file.listFiles();
            if (children != null) for (File child : children) deleteTree(child);
        }
        file.delete();
    }
}
