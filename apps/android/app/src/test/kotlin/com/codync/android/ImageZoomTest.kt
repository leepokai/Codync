package com.codync.android

import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import kotlin.math.abs

class ImageZoomTest {
    private val box = Size(1000f, 800f)
    private val picture = Size(600f, 400f)
    private val big = Size(2000f, 1600f)

    @Test fun `a picture smaller than the box stays centered at fit, and zooming in cannot push it out of the box`() {
        assertEquals(ImageZoom.Fit, ImageZoom(1f, Offset(50f, -20f)).clamped(box, picture))
        // 600 wide at 1.5x is 900: still inside 1000, so no pan.
        assertEquals(ImageZoom(1.5f), ImageZoom(1.5f, Offset(40f, 0f)).clamped(box, picture))
    }

    @Test fun `zoom stays between fit and eight times`() {
        assertEquals(1f, ImageZoom.Fit.zoomedBy(0.2f, Offset.Zero, box, picture).scale)
        assertEquals(8f, ImageZoom.Fit.zoomedBy(100f, Offset.Zero, box, picture).scale)
    }

    @Test fun `a pinch keeps the spot under the fingers where it was`() {
        val point = Offset(300f, -100f)
        val start = ImageZoom(2f, Offset(100f, 50f))
        val after = start.zoomedBy(1.5f, point, box, big)
        // The picture's own coordinate under the fingers, before and after.
        fun under(zoom: ImageZoom) = (point - zoom.offset) / zoom.scale
        assertTrue(abs(under(after).x - under(start).x) < 1e-3f)
        assertTrue(abs(under(after).y - under(start).y) < 1e-3f)
    }

    @Test fun `dragging stops at the picture's edge`() {
        val zoomed = ImageZoom(2f)
        // 2000 * 2 = 4000 wide in a 1000 box: at most 1500 either way.
        assertEquals(1500f, zoomed.pannedBy(Offset(5000f, 0f), box, big).offset.x)
        assertEquals(-1500f, zoomed.pannedBy(Offset(-5000f, 0f), box, big).offset.x)
    }

    @Test fun `a double tap zooms in on the spot, and another goes back to fit`() {
        val zoomed = ImageZoom.Fit.toggled(Offset(100f, 0f), box, big)
        assertEquals(2.5f, zoomed.scale)
        // The spot tapped stays under the finger.
        assertEquals(100f - 100f * 2.5f, zoomed.offset.x)
        assertEquals(ImageZoom.Fit, zoomed.toggled(Offset.Zero, box, big))
    }

    @Test fun `at fit a picture fills the box on its tighter side, keeping its shape`() {
        assertEquals(Size(1000f, 500f), ImageZoom.fitted(Size(4000f, 2000f), box))
        assertEquals(Size(400f, 800f), ImageZoom.fitted(Size(100f, 200f), box))
    }

    @Test fun `a picture panned to its edge leaves no gap after the box grows`() {
        val natural = Size(2000f, 1600f)
        // Twice its fit size in a 1000 x 800 box, moved as far right as it goes.
        val panned = ImageZoom(2f, Offset(500f, 0f)).clamped(box, ImageZoom.fitted(natural, box))
        assertEquals(500f, panned.offset.x)
        // The box doubles in width (split screen): the picture now just fills it across, so it comes back to the middle.
        val wider = Size(2000f, 800f)
        assertEquals(ImageZoom(2f), panned.clamped(wider, ImageZoom.fitted(natural, wider)))
    }

    @Test fun `an animation passes through the zooms in between`() {
        assertEquals(ImageZoom(1.75f, Offset(-50f, 10f)), ImageZoom.Fit.lerp(ImageZoom(2.5f, Offset(-100f, 20f)), 0.5f))
    }
}
