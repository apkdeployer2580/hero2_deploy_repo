package net.wie.emulator;

import android.app.AlertDialog;
import android.content.ClipData;
import android.content.ClipboardManager;
import android.content.Intent;
import android.media.AudioManager;
import android.net.Uri;
import android.os.Build;
import android.os.Bundle;
import android.text.InputType;
import android.view.InputDevice;
import android.view.KeyEvent;
import android.view.MotionEvent;
import android.view.View;
import android.view.Window;
import android.view.WindowInsets;
import android.view.WindowInsetsController;
import android.widget.EditText;
import android.widget.Toast;

import java.io.File;
import java.io.FileInputStream;
import java.io.FileOutputStream;
import java.io.IOException;
import java.io.InputStream;
import java.io.OutputStream;
import java.nio.charset.StandardCharsets;

import androidx.core.content.FileProvider;

import com.google.androidgamesdk.GameActivity;

/**
 * Thin GameActivity subclass. All app logic lives in the Rust cdylib (libmain.so),
 * entered via android_main(). The static loadLibrary + the manifest
 * android.app.lib_name meta-data both point at "main".
 */
public class MainActivity extends GameActivity {
    static {
        System.loadLibrary("main");
    }

    // ndk_context hands native code the *Application*, not the Activity, so we
    // stash the Activity instance here and expose a static entry the Rust side
    // reaches via the app classloader.
    private static MainActivity instance;

    @Override
    protected void onCreate(Bundle savedInstanceState) {
        instance = this;
        // ★ 2026-10-28: before super.onCreate — that is where android_main starts, and the start's
        // own save dialog (an import applied, or not) prints a date.
        try {
            nativeSetAppInfo(java.util.TimeZone.getDefault().getOffset(System.currentTimeMillis()) / 60000, appVersion(this));
        } catch (Throwable ignored) {
            // KST and "version unknown" stay the native defaults.
        }
        super.onCreate(savedInstanceState);
        // Volume rocker -> MEDIA volume, always. Without this the keys control
        // whichever stream Android decides is "active", which for a game that is
        // often silent (menus, dialogue, between songs) is the RINGER — so the
        // player presses volume-up, watches a ring-volume slider go to maximum, and
        // reports that the game is quiet at full volume (2026-08-16).
        setVolumeControlStream(AudioManager.STREAM_MUSIC);
        hideSystemBars();
    }

    /**
     * Fully hide the system bars — most importantly the NAVIGATION bar, which the
     * AppTheme's windowFullscreen does not touch (player request 2026-08-07:
     * "게임이 실행되면 핸드폰의 네비게이션바가 숨김처리 되게").
     *
     * Sticky/transient behaviour on purpose: a swipe from the edge reveals the
     * bars over the game for a moment (so the player can still leave the app),
     * then they hide again by themselves. Re-asserted on every focus gain because
     * the legacy systemUiVisibility flags are CLEARED by the system whenever the
     * bars are shown (dialogs such as ours do exactly that).
     */
    private void hideSystemBars() {
        final Window w = getWindow();
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
            w.setDecorFitsSystemWindows(false);
            WindowInsetsController c = w.getInsetsController();
            if (c != null) {
                c.hide(WindowInsets.Type.systemBars());
                c.setSystemBarsBehavior(
                        WindowInsetsController.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE);
            }
        } else {
            w.getDecorView().setSystemUiVisibility(
                    View.SYSTEM_UI_FLAG_IMMERSIVE_STICKY
                            | View.SYSTEM_UI_FLAG_LAYOUT_STABLE
                            | View.SYSTEM_UI_FLAG_LAYOUT_HIDE_NAVIGATION
                            | View.SYSTEM_UI_FLAG_LAYOUT_FULLSCREEN
                            | View.SYSTEM_UI_FLAG_HIDE_NAVIGATION
                            | View.SYSTEM_UI_FLAG_FULLSCREEN);
        }
    }

    @Override
    public void onWindowFocusChanged(boolean hasFocus) {
        super.onWindowFocusChanged(hasFocus);
        if (hasFocus) {
            hideSystemBars();
        }
    }

    // ── Gamepad (58차) ──────────────────────────────────────────────────────────
    //
    // Captured HERE, at the very top of the dispatch chain, and not left to the
    // GameActivity native input path, because that path was read end-to-end and it
    // cannot carry a gamepad: android_native_app_glue's default_motion_filter drops
    // every non-touchscreen MotionEvent (so joystick axes never reach Rust), and
    // winit would misinterpret any that did as touches. Buttons take the same JNI
    // route so ALL pad input has one owner. Sources are matched with the full-mask
    // idiom — a plain `!= 0` would also catch keyboards via the shared BUTTON class
    // bit. Non-gamepad events fall through to super untouched, so touch, volume,
    // back key and TV-remote D-pads (SOURCE_DPAD only) behave exactly as before.

    private static boolean fromSource(int source, int mask) {
        return (source & mask) == mask;
    }

    private static boolean isGamepadSource(int source) {
        return fromSource(source, InputDevice.SOURCE_GAMEPAD)
                || fromSource(source, InputDevice.SOURCE_JOYSTICK);
    }

    @Override
    public boolean dispatchKeyEvent(KeyEvent e) {
        if (isGamepadSource(e.getSource())) {
            int action = e.getAction();
            if (action == KeyEvent.ACTION_DOWN || action == KeyEvent.ACTION_UP) {
                nativeGamepadButton(e.getKeyCode(), action == KeyEvent.ACTION_DOWN,
                        e.getRepeatCount() > 0);
                return true;
            }
        }
        return super.dispatchKeyEvent(e);
    }

    @Override
    public boolean dispatchGenericMotionEvent(MotionEvent e) {
        if (fromSource(e.getSource(), InputDevice.SOURCE_JOYSTICK)
                && e.getAction() == MotionEvent.ACTION_MOVE) {
            // Left stick + hat (the d-pad on most pads reports as HAT axes, not
            // DPAD key events) + triggers (which many Bluetooth pads report ONLY
            // as axes — LTRIGGER/RTRIGGER or BRAKE/GAS depending on the pad, so
            // take whichever is larger). The Rust side sums and quantises;
            // historical samples inside the event are skipped — the latest
            // position is all a 4-way latch can use.
            nativeGamepadAxes(
                    e.getAxisValue(MotionEvent.AXIS_X), e.getAxisValue(MotionEvent.AXIS_Y),
                    e.getAxisValue(MotionEvent.AXIS_HAT_X), e.getAxisValue(MotionEvent.AXIS_HAT_Y),
                    Math.max(e.getAxisValue(MotionEvent.AXIS_LTRIGGER),
                            e.getAxisValue(MotionEvent.AXIS_BRAKE)),
                    Math.max(e.getAxisValue(MotionEvent.AXIS_RTRIGGER),
                            e.getAxisValue(MotionEvent.AXIS_GAS)));
            return true;
        }
        return super.dispatchGenericMotionEvent(e);
    }

    /** Implemented in Rust — lib.rs `gamepad_jni` (resolved by JNI symbol name). */
    private static native void nativeGamepadButton(int keyCode, boolean down, boolean repeat);
    private static native void nativeGamepadAxes(
            float x, float y, float hatX, float hatY, float lt, float rt);

    /**
     * Change screen orientation from native code. setRequestedOrientation must
     * run on the UI thread. `orientation` = ActivityInfo.SCREEN_ORIENTATION_*
     * (0 landscape, 1 portrait).
     */
    public static void requestOrientation(final int orientation) {
        final MainActivity a = instance;
        if (a == null) {
            return;
        }
        a.runOnUiThread(new Runnable() {
            @Override
            public void run() {
                a.setRequestedOrientation(orientation);
            }
        });
    }

    /**
     * Show on-device diagnostics (settings -> DIAG). Exists because a remote
     * tester cannot run adb: "소리가 안 나요" from another phone is undebuggable
     * unless the key init lines are readable — and copyable — on the device
     * itself. The Rust side passes its half (audio pipeline status, window /
     * logical sizes, mode flags); this side prepends what only Java knows
     * (app version, Android version, device model). 복사 puts the whole text on
     * the clipboard so the tester can paste it into a chat message.
     */
    public static void showDiagnostics(final String rustInfo) {
        final MainActivity a = instance;
        if (a == null) {
            return;
        }
        a.runOnUiThread(new Runnable() {
            @Override
            public void run() {
                String ver;
                try {
                    ver = a.getPackageManager().getPackageInfo(a.getPackageName(), 0).versionName;
                } catch (Exception e) {
                    ver = "?";
                }
                final String text = "app " + ver
                        + "\nandroid " + Build.VERSION.RELEASE + " (API " + Build.VERSION.SDK_INT + ")"
                        + "\ndevice " + Build.MANUFACTURER + " " + Build.MODEL
                        + "\n" + rustInfo;
                new AlertDialog.Builder(a)
                        .setTitle("진단 정보")
                        .setMessage(text + "\n\n소리가 안 나는 등 문제가 있으면 [복사]를 눌러\n개발자에게 붙여넣어 보내주세요.")
                        .setPositiveButton("복사", (dialog, which) -> {
                            ClipboardManager cm = (ClipboardManager) a.getSystemService(CLIPBOARD_SERVICE);
                            if (cm != null) {
                                cm.setPrimaryClip(ClipData.newPlainText("wie diagnostics", text));
                                toast(a, "복사되었습니다");
                            }
                        })
                        .setNegativeButton("닫기", null)
                        // ★ 2026-10-02: 「세이브 관리」 = 다른 폰으로 보내기 · 다른 폰에서 받기 · 백업에서 복원.
                        .setNeutralButton("세이브 관리", (dialog, which) -> showSaveMenu())
                        .show();
            }
        });
    }

    /** Implemented in Rust — lib.rs `vault_jni`. */
    private static native String nativeVaultList();

    private static native boolean nativeVaultRestore(String key);

    /**
     * Offer the save vault's generations and schedule the one the player picks.
     *
     * ── WHY THIS DIALOG EXISTS ─────────────────────────────────────────────
     * Nothing on the device kept a second copy of a hero2 save. The game has
     * three slots and no autosave slot, the host's SAVE button writes straight
     * over the live pair, and a guest can only ever report a slot as empty — so
     * a character that goes wrong is a character that is gone. The vault
     * (wie_android/src/vault.rs) now keeps every version of a save that is about
     * to be overwritten; this is how a player gets one back.
     *
     * It lives behind 진단 정보 rather than on the settings panel deliberately —
     * restoring is destructive, and a row a player could hit by accident next to
     * 발열 절감 would cause the very loss it undoes. The list itself comes from
     * Rust at the moment the dialog opens, because every save rotates the vault.
     *
     * The restore is APPLIED ON THE NEXT LAUNCH: the guest holds the character
     * in its own heap and writes the whole pair back on its next save, so
     * swapping the files underneath a running game would be undone seconds later
     * by the very state the player is trying to get rid of. So the app closes
     * itself after scheduling, and the native side applies it before the
     * emulator starts.
     */
    private static void showSaveRestore() {
        final MainActivity a = instance;
        if (a == null) {
            return;
        }
        String raw;
        try {
            raw = nativeVaultList();
        } catch (Throwable t) {
            raw = null;
        }
        if (raw == null || raw.trim().isEmpty()) {
            new AlertDialog.Builder(a)
                    .setTitle("세이브 복원")
                    .setMessage("보관된 세이브 백업이 없습니다.\n\n백업은 게임이 세이브를 덮어쓸 때마다 자동으로 쌓입니다.")
                    .setPositiveButton("닫기", null)
                    .show();
            return;
        }
        final String[] lines = raw.split("\n");
        final String[] keys = new String[lines.length];
        final String[] labels = new String[lines.length];
        final java.text.SimpleDateFormat fmt =
                new java.text.SimpleDateFormat("MM-dd HH:mm:ss", java.util.Locale.KOREA);
        for (int i = 0; i < lines.length; i++) {
            // `slot|unix_ms|char_len` — savevault::offer_line.
            String[] f = lines[i].split("\\|");
            String slot = f.length > 0 ? f[0] : "?";
            long ms = 0;
            try {
                ms = f.length > 1 ? Long.parseLong(f[1]) : 0;
            } catch (NumberFormatException ignored) {
            }
            String len = f.length > 2 ? f[2] : "?";
            // The key is the first two fields — the length is for the player to
            // read, not for the native side to parse.
            keys[i] = slot + "|" + ms;
            labels[i] = "슬롯 " + slot + " · " + fmt.format(new java.util.Date(ms)) + " · " + len + "B";
        }
        new AlertDialog.Builder(a)
                .setTitle("복원할 백업 선택")
                .setItems(labels, (dialog, which) -> {
                    boolean ok;
                    try {
                        ok = nativeVaultRestore(keys[which]);
                    } catch (Throwable t) {
                        ok = false;
                    }
                    if (!ok) {
                        toast(a, "복원 예약 실패");
                        return;
                    }
                    new AlertDialog.Builder(a)
                            .setTitle("복원 예약됨")
                            .setMessage(labels[which] + "\n\n앱을 닫고 다시 실행하면 복원됩니다.\n지금 닫을까요?")
                            .setPositiveButton("닫기", (d2, w2) -> quitApp())
                            .setNegativeButton("나중에", null)
                            .show();
                })
                .setNegativeButton("취소", null)
                .show();
    }

    /**
     * Close the app so a scheduled restore can be applied on the next launch.
     *
     * `finishAndRemoveTask` alone leaves the process alive with the emulator
     * thread still holding the save open, which is the state the restore must not
     * run in — so the process goes too.
     */
    public static void quitApp() {
        final MainActivity a = instance;
        if (a == null) {
            return;
        }
        a.runOnUiThread(new Runnable() {
            @Override
            public void run() {
                a.finishAndRemoveTask();
                System.exit(0);
            }
        });
    }

    // ── 세이브 옮기기 (2026-10-02) ─────────────────────────────────────────────
    //
    // User: "A핸드폰에서 B핸드폰으로 세이브 데이터를 추출해서 보내고싶어 … 사용자에게도 설명해야해".
    // The saves are app-private, the release build is not debuggable, and allowBackup=false keeps
    // them out of every backup — so the app itself carries them: 내보내기 makes ONE file
    // (wie_android/src/save_transfer.rs), the player sends it however they like (KakaoTalk
    // 나와의 채팅, Quick Share, Drive, a cable), and 가져오기 on the other phone reads it. The import
    // is APPLIED ON THE NEXT LAUNCH for the restore's reason (see showSaveRestore), after the
    // native side has put this phone's own saves into the vault.
    //
    // Java only moves bytes between the system picker / share sheet and a file in the cache dir.
    // What goes into the file, what is accepted and how this phone's saves are kept first is
    // decided natively (vault.rs), in one tested place.

    private static final int REQ_SAVE_EXPORT_FILE = 0x4832;
    private static final int REQ_SAVE_IMPORT_PICK = 0x4833;
    /** The share provider's authority after the package name — AndroidManifest.xml. */
    private static final String SAVE_SHARE_AUTHORITY = ".saveshare";
    /** cache/<this> holds the transfer file — res/xml/save_share_paths.xml serves exactly it. */
    private static final String SAVE_SHARE_DIR = "save_share";
    /** Above any transfer file (the native side refuses past 4 MB). */
    private static final long IMPORT_MAX_BYTES = 8L * 1024 * 1024;

    /** Implemented in Rust — lib.rs `vault_jni`. Each answers "ok…" or "err|<a sentence>". */
    private static native String nativeSaveExport(String filesDir, String outPath, int appVersion);

    private static native String nativeSaveImportCheck(String filesDir, String inPath, int appVersion);

    private static native String nativeSaveImportSchedule(String filesDir, String inPath, int appVersion);

    /** 진단 정보 → 세이브 관리: the three things a player does with saves. */
    private static void showSaveMenu() {
        final MainActivity a = instance;
        if (a == null) {
            return;
        }
        final String[] items = {
                "다른 폰으로 보내기 (내보내기)",
                "다른 폰에서 받기 (가져오기)",
                "백업에서 복원",
        };
        new AlertDialog.Builder(a)
                .setTitle("세이브 관리")
                .setItems(items, (dialog, which) -> {
                    if (which == 0) {
                        showSaveExport(a);
                    } else if (which == 1) {
                        showSaveImport(a);
                    } else {
                        showSaveRestore();
                    }
                })
                .setNegativeButton("닫기", null)
                .show();
    }

    private static void showSaveExport(final MainActivity a) {
        new AlertDialog.Builder(a)
                .setTitle("세이브 내보내기")
                .setMessage("이 폰의 세이브(슬롯 1~5)·계정창고·아바타·퀵슬롯을 파일 하나로 만듭니다.\n\n"
                        + "⚠ 마지막으로 저장(SAVE)한 상태가 들어갑니다. 먼저 게임에서 저장하세요.\n\n"
                        + "[보내기] 카카오톡·Quick Share 등으로 바로 보냅니다.\n"
                        + "[파일로 저장] 이 폰의 폴더(다운로드 등)에 저장합니다.")
                .setPositiveButton("보내기", (dialog, which) -> shareExport(a))
                .setNeutralButton("파일로 저장", (dialog, which) -> pickExportFile(a))
                .setNegativeButton("취소", null)
                .show();
    }

    /** Make the transfer file in the cache dir; null (after saying why) when it could not be made. */
    private static File exportToCache(MainActivity a) {
        File dir = new File(a.getCacheDir(), SAVE_SHARE_DIR);
        // One file at a time: an older export left here is a stale copy nobody should send.
        File[] old = dir.listFiles();
        if (old != null) {
            for (File f : old) {
                //noinspection ResultOfMethodCallIgnored
                f.delete();
            }
        }
        //noinspection ResultOfMethodCallIgnored
        dir.mkdirs();
        String name = "영웅서기2_세이브_"
                + new java.text.SimpleDateFormat("yyyyMMdd_HHmm", java.util.Locale.KOREA).format(new java.util.Date())
                + ".zip";
        File out = new File(dir, name);
        String r;
        try {
            r = nativeSaveExport(a.getFilesDir().getAbsolutePath(), out.getAbsolutePath(), appVersion(a));
        } catch (Throwable t) {
            r = null;
        }
        if (r == null || !r.startsWith("ok")) {
            showSaveResult(a, "내보내기 실패", reason(r, "세이브 파일을 만들지 못했습니다."));
            return null;
        }
        return out;
    }

    private static void shareExport(MainActivity a) {
        File out = exportToCache(a);
        if (out == null) {
            return;
        }
        try {
            Uri uri = FileProvider.getUriForFile(a, a.getPackageName() + SAVE_SHARE_AUTHORITY, out);
            Intent send = new Intent(Intent.ACTION_SEND);
            send.setType("application/zip");
            send.putExtra(Intent.EXTRA_STREAM, uri);
            // ClipData as well: since API 29 that is what carries the read grant through the
            // chooser to the app the player picks.
            send.setClipData(ClipData.newRawUri(out.getName(), uri));
            send.addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION);
            a.startActivity(Intent.createChooser(send, "세이브 파일 보내기"));
        } catch (Throwable t) {
            showSaveResult(a, "보내기 실패", "공유 창을 열지 못했습니다. [파일로 저장]으로 저장한 뒤 보내 주세요.");
        }
    }

    private static void pickExportFile(MainActivity a) {
        // Made BEFORE the picker opens, so a failure is said here — not after the player has
        // already created an empty file somewhere.
        File out = exportToCache(a);
        if (out == null) {
            return;
        }
        Intent pick = new Intent(Intent.ACTION_CREATE_DOCUMENT);
        pick.addCategory(Intent.CATEGORY_OPENABLE);
        pick.setType("application/zip");
        pick.putExtra(Intent.EXTRA_TITLE, out.getName());
        try {
            a.startActivityForResult(pick, REQ_SAVE_EXPORT_FILE);
        } catch (Throwable t) {
            showSaveResult(a, "파일로 저장 실패", "파일 저장 창을 열지 못했습니다. [보내기]를 써 주세요.");
        }
    }

    /** The picker came back with a place to save: copy the file made before it opened. */
    private static void finishExportFile(MainActivity a, Uri dest) {
        // The newest file there — the system may have recreated the process while the picker was
        // open, and the cache survives that. Made again when it did not.
        File src = newestFile(new File(a.getCacheDir(), SAVE_SHARE_DIR));
        if (src == null) {
            src = exportToCache(a);
            if (src == null) {
                return;
            }
        }
        try (InputStream in = new FileInputStream(src);
             OutputStream out = a.getContentResolver().openOutputStream(dest)) {
            if (out == null) {
                throw new IOException("no output stream");
            }
            copy(in, out, Long.MAX_VALUE);
        } catch (Throwable t) {
            showSaveResult(a, "파일로 저장 실패", "파일을 쓰지 못했습니다. 다른 폴더를 골라 다시 해 보세요.");
            return;
        }
        showSaveResult(a, "저장했습니다", src.getName()
                + "\n\n이 파일을 다른 폰으로 보낸 뒤, 그 폰의 게임에서\n환경설정 → 세이브불러오기\n를 누르세요.");
    }

    private static void showSaveImport(final MainActivity a) {
        new AlertDialog.Builder(a)
                .setTitle("세이브 가져오기")
                .setMessage("다른 폰에서 내보낸 세이브 파일(영웅서기2_세이브_….zip)을 고르세요.\n\n"
                        + "⚠ 가져오면 이 폰의 세이브(슬롯 1~5)·계정창고·아바타·퀵슬롯이 모두 파일 내용으로 바뀝니다. "
                        + "지금 이 폰의 세이브는 백업으로 남아 게임의 환경설정 → 세이브되돌리기로 되돌릴 수 있습니다.")
                .setPositiveButton("파일 고르기", (dialog, which) -> {
                    Intent pick = new Intent(Intent.ACTION_OPEN_DOCUMENT);
                    pick.addCategory(Intent.CATEGORY_OPENABLE);
                    // Any type: a file that came through a messenger is not always labelled a zip.
                    // The native side decides what it is.
                    pick.setType("*/*");
                    try {
                        a.startActivityForResult(pick, REQ_SAVE_IMPORT_PICK);
                    } catch (Throwable t) {
                        showSaveResult(a, "가져오기 실패", "파일 선택 창을 열지 못했습니다.");
                    }
                })
                .setNegativeButton("취소", null)
                .show();
    }

    /** The picked file: copy it in, let the native side read it, and ask with what it found. */
    private static void importPicked(final MainActivity a, Uri src) {
        File dir = new File(a.getCacheDir(), "save_import");
        //noinspection ResultOfMethodCallIgnored
        dir.mkdirs();
        final File cand = new File(dir, "candidate.zip");
        try (InputStream in = a.getContentResolver().openInputStream(src);
             OutputStream out = new FileOutputStream(cand)) {
            if (in == null) {
                throw new IOException("no input stream");
            }
            copy(in, out, IMPORT_MAX_BYTES);
        } catch (Throwable t) {
            //noinspection ResultOfMethodCallIgnored
            cand.delete();
            showSaveResult(a, "가져오기 실패", "파일을 읽지 못했습니다. 영웅서기2 세이브 파일인지 확인해 주세요.");
            return;
        }
        String r;
        try {
            r = nativeSaveImportCheck(a.getFilesDir().getAbsolutePath(), cand.getAbsolutePath(), appVersion(a));
        } catch (Throwable t) {
            r = null;
        }
        if (r == null || !r.startsWith("ok")) {
            //noinspection ResultOfMethodCallIgnored
            cand.delete();
            showSaveResult(a, "가져오기 실패", reason(r, "파일을 확인하지 못했습니다."));
            return;
        }
        // ok · created|ms · app|vc · file|<slot>… · extra|… · here|<slot>… — vault::import_summary.
        long created = 0;
        StringBuilder file = new StringBuilder();
        StringBuilder here = new StringBuilder();
        String extra = null;
        for (String line : r.split("\n")) {
            int bar = line.indexOf('|');
            if (bar < 0) {
                continue;
            }
            String value = line.substring(bar + 1);
            switch (line.substring(0, bar)) {
                case "created":
                    try {
                        created = Long.parseLong(value);
                    } catch (NumberFormatException ignored) {
                        created = 0;
                    }
                    break;
                case "file":
                    file.append("\n  ").append(value);
                    break;
                case "extra":
                    extra = value;
                    break;
                case "here":
                    here.append("\n  ").append(value);
                    break;
                default:
                    break;
            }
        }
        StringBuilder msg = new StringBuilder("받은 파일");
        if (created > 0) {
            msg.append(" (")
                    .append(new java.text.SimpleDateFormat("M월 d일 HH:mm", java.util.Locale.KOREA).format(new java.util.Date(created)))
                    .append(" 내보냄)");
        }
        msg.append(":").append(file);
        if (extra != null) {
            msg.append("\n  + ").append(extra);
        }
        msg.append("\n\n이 폰의 지금 세이브 (백업으로 보관됨):");
        msg.append(here.length() > 0 ? here.toString() : "\n  없음");
        msg.append("\n\n[가져오기]를 누르면 앱이 닫힙니다. 다시 실행하면 받은 세이브로 시작합니다.");
        new AlertDialog.Builder(a)
                .setTitle("이 세이브를 가져올까요?")
                .setMessage(msg.toString())
                .setPositiveButton("가져오기", (dialog, which) -> scheduleImport(a, cand))
                .setNegativeButton("취소", (dialog, which) -> cand.delete())
                .show();
    }

    private static void scheduleImport(MainActivity a, File cand) {
        String r;
        try {
            r = nativeSaveImportSchedule(a.getFilesDir().getAbsolutePath(), cand.getAbsolutePath(), appVersion(a));
        } catch (Throwable t) {
            r = null;
        }
        //noinspection ResultOfMethodCallIgnored
        cand.delete();
        if (r == null || !r.startsWith("ok")) {
            showSaveResult(a, "가져오기 실패", reason(r, "가져오기를 준비하지 못했습니다."));
            return;
        }
        // No 「나중에」 here (the restore offers one): a save made before the relaunch would only be
        // run over by the import, so closing now is the one honest choice.
        new AlertDialog.Builder(a)
                .setTitle("가져오기 준비 완료")
                .setMessage("앱을 닫습니다.\n다시 실행하면 받은 세이브로 시작합니다.")
                .setPositiveButton("확인", (dialog, which) -> quitApp())
                .setCancelable(false)
                .show();
    }

    /**
     * What the boot did with a scheduled import — the native side calls this on the first frame
     * (server_prompt.rs `show_save_message`). The first line is the title.
     */
    public static void showSaveMessage(final String text) {
        final MainActivity a = instance;
        if (a == null || text == null) {
            return;
        }
        a.runOnUiThread(() -> {
            int nl = text.indexOf('\n');
            showSaveResult(a, nl > 0 ? text.substring(0, nl) : "세이브", nl > 0 ? text.substring(nl + 1) : text);
        });
    }

    private static void showSaveResult(MainActivity a, String title, String message) {
        new AlertDialog.Builder(a)
                .setTitle(title)
                .setMessage(message)
                .setPositiveButton("확인", null)
                .show();
    }

    /** The sentence after `err|`, or `fallback` when the native call gave none. */
    private static String reason(String r, String fallback) {
        return r != null && r.startsWith("err|") && r.length() > 4 ? r.substring(4) : fallback;
    }

    @SuppressWarnings("deprecation")
    private static int appVersion(MainActivity a) {
        try {
            android.content.pm.PackageInfo p = a.getPackageManager().getPackageInfo(a.getPackageName(), 0);
            return Build.VERSION.SDK_INT >= Build.VERSION_CODES.P ? (int) p.getLongVersionCode() : p.versionCode;
        } catch (Throwable t) {
            return 0;   // unknown: the native side then skips the newer-app check
        }
    }

    private static File newestFile(File dir) {
        File[] files = dir.listFiles();
        File best = null;
        if (files != null) {
            for (File f : files) {
                if (f.isFile() && (best == null || f.lastModified() > best.lastModified())) {
                    best = f;
                }
            }
        }
        return best;
    }

    /** Copy everything, refusing past `max` bytes. */
    private static void copy(InputStream in, OutputStream out, long max) throws IOException {
        byte[] buf = new byte[16 * 1024];
        long total = 0;
        int n;
        while ((n = in.read(buf)) != -1) {
            total += n;
            if (total > max) {
                throw new IOException("over " + max + " bytes");
            }
            out.write(buf, 0, n);
        }
    }

    // ── 환경설정의 세이브 행 (2026-10-28) ──────────────────────────────────────────
    //
    // User: "hero5와 cronossword 처럼 인게임 내에 환경설정에서 맨밑에 행 3개 추가해서 세이브 내보내기, 세이브
    // 불러오기, 세이브 되돌리기". The rows and their dialogs are the native side's (in the game's own
    // window and boxes); Java only opens the system's file screens and copies bytes, and answers
    // nativeSaveDocResult(id, status) EXACTLY ONCE per request — every path, including a screen that
    // could not open and a copy that threw, goes through answer(). The native side takes only the
    // answer of the request it is waiting for (chronos review 2: a cloud file is downloaded inside
    // openInputStream, so a copy can outlive the screen, and its late answer must not be read as the
    // next request's), each request has its own file, and the copy runs off the UI thread.

    /** Implemented in Rust — lib.rs `vault_jni`. */
    private static native void nativeSetAppInfo(int offsetMinutes, int versionCode);

    private static native void nativeSaveDocResult(int id, int status);

    private static final int DOC_DONE = 0;
    private static final int DOC_CANCELLED = 1;
    private static final int DOC_FAILED = 2;
    private static final int DOC_TOO_BIG = 3;
    /** One byte over the native ceiling (save_transfer::FILE_MAX): a bigger file is no save. */
    private static final long DOC_IMPORT_MAX = 4L * 1024 * 1024 + 1;
    private static final int REQ_DOC_EXPORT = 0x4000;
    private static final int REQ_DOC_IMPORT = 0x5000;

    /** A request in flight: its native id, its kind and its file. Keyed by the request code. */
    private static final class DocRequest {
        final int id;
        final boolean export;
        final File file;

        DocRequest(int id, boolean export, File file) {
            this.id = id;
            this.export = export;
            this.file = file;
        }
    }

    private static final java.util.Map<Integer, DocRequest> docRequests = new java.util.HashMap<>();

    private static void answer(int id, int status) {
        try {
            nativeSaveDocResult(id, status);
        } catch (Throwable ignored) {
            // The native side gives a request up after its own patience runs out.
        }
    }

    /**
     * Open a file screen for the native side: `export|<id>|<file>|<suggested name>` or
     * `import|<id>|<file>`. Called from the emulator thread (server_prompt.rs `save_doc`).
     */
    public static void saveDoc(final String spec) {
        final String[] f = spec == null ? new String[0] : spec.split("\\|", 4);
        int parsed;
        try {
            parsed = f.length >= 3 ? Integer.parseInt(f[1]) : -1;
        } catch (NumberFormatException e) {
            parsed = -1;
        }
        final int id = parsed;
        final MainActivity a = instance;
        if (id < 0 || a == null || !("export".equals(f[0]) || "import".equals(f[0]))) {
            if (id >= 0) {
                answer(id, DOC_FAILED);
            }
            return;
        }
        final boolean export = "export".equals(f[0]);
        final File file = new File(f[2]);
        final String name = f.length > 3 ? f[3] : file.getName();
        a.runOnUiThread(() -> {
            int code = (export ? REQ_DOC_EXPORT : REQ_DOC_IMPORT) | (id & 0xfff);
            Intent pick;
            if (export) {
                pick = new Intent(Intent.ACTION_CREATE_DOCUMENT);
                pick.addCategory(Intent.CATEGORY_OPENABLE);
                // Not application/zip: some providers then append their own ".zip".
                pick.setType("application/octet-stream");
                pick.putExtra(Intent.EXTRA_TITLE, name);
            } else {
                pick = new Intent(Intent.ACTION_OPEN_DOCUMENT);
                pick.addCategory(Intent.CATEGORY_OPENABLE);
                // Any type: a file that came through a messenger is not always labelled a zip.
                pick.setType("*/*");
            }
            synchronized (docRequests) {
                docRequests.put(code, new DocRequest(id, export, file));
            }
            try {
                a.startActivityForResult(pick, code);
            } catch (Throwable t) {
                synchronized (docRequests) {
                    docRequests.remove(code);
                }
                answer(id, DOC_FAILED);
            }
        });
    }

    /** The file screen came back for request `req`: copy on a thread of its own, then answer. */
    private static void finishDoc(final MainActivity a, final DocRequest req, final Uri uri) {
        new Thread(() -> {
            int status = DOC_FAILED;
            try {
                if (req.export) {
                    try (InputStream in = new FileInputStream(req.file);
                         OutputStream out = openTruncating(a, uri)) {
                        copy(in, out, Long.MAX_VALUE);
                    }
                    status = DOC_DONE;
                } else {
                    File tmp = new File(req.file.getPath() + ".part");
                    //noinspection ResultOfMethodCallIgnored
                    req.file.getParentFile().mkdirs();
                    try (InputStream in = a.getContentResolver().openInputStream(uri);
                         OutputStream out = new FileOutputStream(tmp)) {
                        if (in == null) {
                            throw new IOException("no input stream");
                        }
                        copy(in, out, DOC_IMPORT_MAX - 1);
                    } catch (IOException e) {
                        //noinspection ResultOfMethodCallIgnored
                        tmp.delete();
                        if (e.getMessage() != null && e.getMessage().startsWith("over ")) {
                            status = DOC_TOO_BIG;
                        }
                        throw e;
                    }
                    if (!tmp.renameTo(req.file)) {
                        //noinspection ResultOfMethodCallIgnored
                        tmp.delete();
                        throw new IOException("rename");
                    }
                    status = DOC_DONE;
                }
            } catch (Exception e) {
                // status says which failure it was
            } finally {
                answer(req.id, status);
            }
        }, "hero2-save-doc").start();
    }

    /** "wt" truncates what the player chose to overwrite; a provider that knows only "w" gets that. */
    private static OutputStream openTruncating(MainActivity a, Uri uri) throws IOException {
        OutputStream out;
        try {
            out = a.getContentResolver().openOutputStream(uri, "wt");
        } catch (IllegalArgumentException | UnsupportedOperationException e) {
            out = a.getContentResolver().openOutputStream(uri);
        }
        if (out == null) {
            throw new IOException("no output stream");
        }
        return out;
    }

    /**
     * The start whose staged save could not be made one whole save (vault.rs BLOCKED_TEXT): the
     * game does not run, and the dialog's only button closes the app. The first line is the title.
     */
    public static void showBlocked(final String text) {
        final MainActivity a = instance;
        if (a == null) {
            System.exit(0);
            return;
        }
        a.runOnUiThread(() -> {
            int nl = text == null ? -1 : text.indexOf('\n');
            new AlertDialog.Builder(a)
                    .setTitle(nl > 0 ? text.substring(0, nl) : "게임을 시작하지 않습니다")
                    .setMessage(nl > 0 ? text.substring(nl + 1) : String.valueOf(text))
                    .setCancelable(false)
                    .setPositiveButton("닫기", (dialog, which) -> quitApp())
                    .show();
        });
    }

    /** quitApp for the native side's one-String-argument call path (server_prompt.rs `quit_app`). */
    public static void quitAppWith(String ignored) {
        if (instance == null) {
            System.exit(0);
            return;
        }
        quitApp();
    }

    /** The system file pickers' answers for 세이브 옮기기 and for the 환경설정 rows. */
    @Override
    @SuppressWarnings("deprecation")
    protected void onActivityResult(int requestCode, int resultCode, Intent data) {
        super.onActivityResult(requestCode, resultCode, data);
        if ((requestCode & 0xf000) == REQ_DOC_EXPORT || (requestCode & 0xf000) == REQ_DOC_IMPORT) {
            DocRequest req;
            synchronized (docRequests) {
                req = docRequests.remove(requestCode);
            }
            if (req == null) {
                // A request of a process the system recreated under the screen: the native side that
                // asked is gone too, and nothing waits for the answer.
                return;
            }
            if (resultCode != RESULT_OK || data == null || data.getData() == null) {
                answer(req.id, DOC_CANCELLED);
                return;
            }
            finishDoc(this, req, data.getData());
            return;
        }
        if (resultCode != RESULT_OK || data == null || data.getData() == null) {
            return;
        }
        if (requestCode == REQ_SAVE_EXPORT_FILE) {
            finishExportFile(this, data.getData());
        } else if (requestCode == REQ_SAVE_IMPORT_PICK) {
            importPicked(this, data.getData());
        }
    }

    /** File the Rust side reads the private-server endpoint from (config.rs). */
    private static final String SERVER_FILE = "wie_server.txt";

    /**
     * Ask the player for the private-server address and persist it where the
     * native side looks for it.
     *
     * Done in Java on purpose: this needs a real text field (hostnames contain
     * letters, dots and digits), and the in-game overlay draws with a 5x7 ASCII
     * bitmap font with no text-input machinery at all. Going through the platform
     * dialog gets the soft keyboard, clipboard paste and IME for free, and the
     * native side stays a plain file read — the same shape as every other
     * persisted setting.
     *
     * `current` is shown pre-filled so the player edits rather than retypes.
     * Writing the file (instead of calling back into Rust) keeps this one-way:
     * `network.rs` re-reads the endpoint on the game's next connect, so there is
     * no state to keep in sync and no restart needed.
     */
    public static void promptServerAddress(final String current) {
        final MainActivity a = instance;
        if (a == null) {
            return;
        }
        a.runOnUiThread(new Runnable() {
            @Override
            public void run() {
                final EditText input = new EditText(a);
                // URI type = no autocorrect/capitalisation mangling a hostname.
                input.setInputType(InputType.TYPE_TEXT_VARIATION_URI);
                input.setSingleLine(true);
                input.setHint("tls://hero3.example.com:8091");
                if (current != null) {
                    input.setText(current);
                    input.setSelection(current.length());
                }
                new AlertDialog.Builder(a)
                        .setTitle("서버 주소")
                        .setMessage("접속할 사설 서버의 주소를 입력하세요.\n\n"
                                + "인터넷의 공개 서버:  tls://주소:8091\n"
                                + "  → tls:// 를 붙이면 통신이 암호화됩니다. 계정 정보가\n"
                                + "     그대로 오가므로 공개 서버에는 반드시 붙이세요.\n"
                                + "내 PC 개발용:  127.0.0.1\n"
                                + "(포트를 생략하면 8091)\n\n"
                                + "⚠ 믿을 수 있는 서버만 입력하세요. 서버는 게임에 데이터를 보냅니다.")
                        .setView(input)
                        .setPositiveButton("저장", (dialog, which) -> {
                            String value = input.getText().toString().trim();
                            if (value.isEmpty()) {
                                return;
                            }
                            // Validate the SHAPE here. The native side falls back to
                            // the built-in address when the file does not parse, so
                            // without this a typo would be "saved" and then silently
                            // ignored — the worst of both messages.
                            if (!looksLikeEndpoint(value)) {
                                toast(a, "주소 형식이 올바르지 않습니다\n예: tls://hero3.example.com:8091");
                                return;
                            }
                            if (writeTextFile(a, SERVER_FILE, value)) {
                                toast(a, "서버 주소 저장: " + value + "\n다음 접속부터 적용됩니다");
                            } else {
                                toast(a, "저장 실패");
                            }
                        })
                        .setNeutralButton("기본값", (dialog, which) -> {
                            // Delete the override so config.rs falls back to the
                            // build-time host (or localhost for the dev loop).
                            new File(a.getFilesDir(), SERVER_FILE).delete();
                            toast(a, "기본 서버로 되돌렸습니다");
                        })
                        .setNegativeButton("취소", null)
                        .show();
            }
        });
    }

    /**
     * Loose shape check mirroring `endpoint.rs::parse_endpoint`: an optional
     * scheme, a hostname/IP made of letters, digits, dot, dash, underscore or
     * colon (IPv6), and an optional :port. Intentionally permissive — the native
     * parser is authoritative — it only exists to catch obvious typos before the
     * user is told the value was saved.
     */
    private static boolean looksLikeEndpoint(String v) {
        String s = v;
        int scheme = s.indexOf("://");
        if (scheme >= 0) {
            s = s.substring(scheme + 3);
        }
        while (s.endsWith("/")) {
            s = s.substring(0, s.length() - 1);
        }
        if (s.isEmpty() || s.length() > 253) {
            return false;
        }
        // Trailing :digits = port. Only strip it when it really is a port, so an
        // IPv6 literal (which is all colons) is not mangled.
        int colon = s.lastIndexOf(':');
        if (colon > 0 && colon < s.length() - 1 && s.indexOf(':') == colon) {
            String port = s.substring(colon + 1);
            if (port.matches("\\d{1,5}")) {
                int p = Integer.parseInt(port);
                if (p < 1 || p > 65535) {
                    return false;
                }
                s = s.substring(0, colon);
            }
        }
        return !s.isEmpty() && s.matches("[A-Za-z0-9._:\\[\\]-]+");
    }

    /** File holding the 16-byte account identity (config.rs). */
    private static final String ACCOUNT_FILE = "wie_account_id";

    /**
     * Show the account code and allow replacing it with one from another device.
     *
     * This is the ONLY way a player can keep their network warehouse / character
     * backups across a reinstall or a new phone: the server recognises nobody but
     * these 16 bytes, and they live only in this app's private storage. Without
     * this dialog, "I changed phones" means the data is gone for good.
     *
     * It is also the credential, which is why the text says so plainly rather than
     * calling it a "code" and leaving the user to guess.
     */
    public static void promptAccountCode(final String currentHex) {
        final MainActivity a = instance;
        if (a == null) {
            return;
        }
        a.runOnUiThread(new Runnable() {
            @Override
            public void run() {
                final EditText input = new EditText(a);
                input.setInputType(InputType.TYPE_TEXT_VARIATION_PASSWORD
                        | InputType.TYPE_CLASS_TEXT);
                input.setSingleLine(false);
                if (currentHex != null) {
                    input.setText(currentHex);
                    input.setSelection(currentHex.length());
                }
                new AlertDialog.Builder(a)
                        .setTitle("계정 코드")
                        .setMessage("이 코드가 서버에서 당신의 계정입니다.\n\n"
                                + "• 새 휴대폰으로 옮길 때: 이 코드를 복사해 두었다가\n"
                                + "  새 기기에서 여기에 붙여넣으세요.\n"
                                + "• 앱을 삭제하면 코드가 사라지고, 창고·복구 데이터에\n"
                                + "  다시 접근할 수 없습니다. 미리 복사해 두세요.\n\n"
                                + "⚠ 이 코드는 비밀번호입니다. 남에게 보여주면 그 사람이\n"
                                + "   당신의 창고와 캐릭터 백업을 가져갈 수 있습니다.")
                        .setView(input)
                        .setPositiveButton("변경", (dialog, which) -> {
                            String hex = input.getText().toString()
                                    .replaceAll("[\\s:-]", "").toLowerCase();
                            if (!hex.matches("[0-9a-f]{32}")) {
                                toast(a, "코드는 32자리 16진수여야 합니다");
                                return;
                            }
                            if (hex.equals(currentHex)) {
                                return;     // unchanged — nothing to do
                            }
                            if (writeAccountId(a, hex)) {
                                toast(a, "계정 코드를 변경했습니다\n다음 로그인부터 이 계정으로 연결됩니다");
                            } else {
                                toast(a, "저장 실패");
                            }
                        })
                        .setNegativeButton("닫기", null)
                        .show();
            }
        });
    }

    /** Write 32 hex chars as the raw 16 bytes `config.rs` expects. */
    private static boolean writeAccountId(MainActivity a, String hex) {
        byte[] id = new byte[16];
        for (int i = 0; i < 16; i++) {
            id[i] = (byte) Integer.parseInt(hex.substring(i * 2, i * 2 + 2), 16);
        }
        File f = new File(a.getFilesDir(), ACCOUNT_FILE);
        try (FileOutputStream out = new FileOutputStream(f)) {
            out.write(id);
            return true;
        } catch (Exception e) {
            return false;
        }
    }

    private static boolean writeTextFile(MainActivity a, String name, String value) {
        File f = new File(a.getFilesDir(), name);
        try (FileOutputStream out = new FileOutputStream(f)) {
            out.write((value + "\n").getBytes(StandardCharsets.UTF_8));
            return true;
        } catch (Exception e) {
            return false;
        }
    }

    private static void toast(MainActivity a, String msg) {
        Toast.makeText(a, msg, Toast.LENGTH_LONG).show();
    }
}
