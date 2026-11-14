package com.codync.android;

import android.app.Activity;
import android.app.Instrumentation;
import android.content.res.AssetManager;
import android.os.Build;
import android.os.Bundle;
import java.io.ByteArrayOutputStream;
import java.io.File;
import java.io.FileOutputStream;
import java.io.InputStream;
import java.nio.ByteBuffer;
import java.nio.ByteOrder;
import java.nio.charset.StandardCharsets;
import java.util.Arrays;
import com.sun.jna.Pointer;
import org.json.JSONObject;
import org.vosk.LibVosk;
import org.vosk.Model;

/** A platform-only runner exercises the actual R8 native binding without test-library keep rules. */
public final class VoiceReleaseRunner extends Instrumentation {
    @Override public void onCreate(Bundle arguments) { super.onCreate(arguments); start(); }
    @Override public void onStart() {
        Bundle result = new Bundle();
        File temporary = new File(getTargetContext().getCacheDir(), "release-voice-smoke");
        try {
            if (!Build.PRODUCT.contains("sdk")) throw new IllegalStateException("Use an emulator for release acceptance");
            temporary.mkdirs();
            copy(getTargetContext().getAssets(), "voice/vosk-model-small-en-us-0.15", temporary);
            byte[] wav;
            try (InputStream input = getContext().getAssets().open("voice-test.wav")) {
                ByteArrayOutputStream bytes = new ByteArrayOutputStream();
                byte[] buffer = new byte[8192]; int count;
                while ((count = input.read(buffer)) != -1) bytes.write(buffer, 0, count);
                wav = bytes.toByteArray();
            }
            ByteBuffer header = ByteBuffer.wrap(wav).order(ByteOrder.LITTLE_ENDIAN);
            byte[] pcm = null;
            for (int offset = 12; offset + 8 <= wav.length;) {
                int length = header.getInt(offset + 4);
                if (length < 0 || length > wav.length - offset - 8) throw new IllegalStateException("Invalid speech fixture");
                if (new String(wav, offset, 4, StandardCharsets.US_ASCII).equals("data")) pcm = Arrays.copyOfRange(wav, offset + 8, offset + 8 + length);
                offset += 8 + length + length % 2;
            }
            if (pcm == null) throw new IllegalStateException("Missing fixture audio");
            LibVosk.vosk_set_log_level(-1);
            StringBuilder heard = new StringBuilder();
            Model model = new Model();
            model.setPointer(LibVosk.vosk_model_new(temporary.getAbsolutePath()));
            if (model.getPointer() == null) throw new IllegalStateException("The minified native model did not load");
            Pointer recognizer = null;
            try {
                recognizer = LibVosk.vosk_recognizer_new(model, 16000f);
                if (recognizer == null) throw new IllegalStateException("The minified native recognizer did not load");
                for (int position = 0; position < pcm.length; position += 4096) {
                    byte[] chunk = Arrays.copyOfRange(pcm, position, Math.min(position + 4096, pcm.length));
                    if (LibVosk.vosk_recognizer_accept_waveform(recognizer, chunk, chunk.length)) heard.append(new JSONObject(LibVosk.vosk_recognizer_result(recognizer)).optString("text")).append(' ');
                }
                heard.append(new JSONObject(LibVosk.vosk_recognizer_final_result(recognizer)).optString("text"));
            } finally {
                if (recognizer != null) LibVosk.vosk_recognizer_free(recognizer);
                LibVosk.vosk_model_free(model.getPointer());
            }
            if (!heard.toString().contains("one zero zero zero one") || !heard.toString().contains("zero one eight zero three")) throw new AssertionError("Minified speech recognition did not match the fixture");
            Class<?> widgets = Class.forName("com.codync.android.ReleaseWidgetSmoke", true, getTargetContext().getClassLoader());
            widgets.getMethod("run", Instrumentation.class).invoke(null, this);
            result.putString("stream", "\nOK (2 tests)\n");
            finish(Activity.RESULT_OK, result);
        } catch (Throwable error) {
            Throwable detail = error instanceof java.lang.reflect.InvocationTargetException && error.getCause() != null ? error.getCause() : error;
            result.putString("stream", "\nFAILURES!!!\n" + detail.getClass().getSimpleName() + ": " + detail.getMessage() + "\n");
            finish(Activity.RESULT_CANCELED, result);
        } finally { erase(temporary); }
    }
    private void copy(AssetManager assets, String source, File target) throws Exception {
        for (String child : assets.list(source)) {
            String path = source + "/" + child; File output = new File(target, child);
            if (assets.list(path).length > 0) { output.mkdirs(); copy(assets, path, output); }
            else try (InputStream input = assets.open(path); FileOutputStream destination = new FileOutputStream(output)) {
                byte[] buffer = new byte[8192]; int count;
                while ((count = input.read(buffer)) != -1) destination.write(buffer, 0, count);
            }
        }
    }
    private void erase(File file) {
        File[] children = file.listFiles();
        if (children != null) for (File child : children) erase(child);
        file.delete();
    }
}
