package moe.tarsin.ehviewer.rust.prototype;

import com.google.androidgamesdk.GameActivity;

/**
 * Platform loading bridge only. Screens, interaction state and effects belong to Rust.
 * GameActivity supplies Android lifecycle, IME and SurfaceView for AccessKit.
 */
public final class BootstrapActivity extends GameActivity {
    static {
        System.loadLibrary("eh_native");
    }
}
