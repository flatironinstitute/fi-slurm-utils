The TUI is due for a considerable refactor: at present, far too much work is done 'per-frame', including computations and resizing. These should be moved into the initial setup, with a dedicated ui function for re-sizing the display, rather than dynamically detecting the size of the display and calculating the number of bars to show on each frame.

This refactor would also solve, or make it much easier to solve, the major bug in the display: that the bars do not all appear as desired for all resolutions and terminal sizes. At present, this bug can be fixed, but not without re-introducing other bugs in the scroll mechanism owing to misalignment between initial and per-frame data.

Some details of the problem follow:

# The Problem:
At present, the state management in app.rs is tightly coupled to the rendering logic in ui.rs.

The event loop attempts to predict the geometry of the UI to manage the horizontal and vertical scroll behavior, and must determine how and whether to adjust the bars being displayed on the screen in response to user input.

However, the actual width of the chart is only known to the rendering function. 

At present, the number of bars displayed is inconsistent, and the MAX bar in particular is not always displayed at some resolutions.

Attempts to fix this have led to other bugs, in which the state manager's assumptions about the geometry conflict with the rendered frame, leading to off-by-one errors and the impression that the TUI is 'stuck'.

# Refactor guide

A refactored TUI should separate concerns and stop trying to duplicate work: in particular, the state manager should only manage the data and communicate user intent to the renderer, while the renderer should be a single source of truth for all display logic.

The horizontal scroll offset should be redefined as representing the rightmost visible data point, instead of the leftmost visible bar. It is the state manager's job to change this index, if the change would be valid.

The renderer should perform the following per-frame steps: 

Get Actual Width: Determine the true inner width of the chart's drawing area from the Rect provided by the layout engine.

Calculate Visible Bars: Based on this width, calculate exactly how many bars (num_to_display) can fit.

Determine the Slice: Use the horizontal_scroll_offset (the end of the slice) and num_to_display (the length of the slice) to determine the starting index of the data to be rendered.

Render: Slice the data array using these calculated start and end points and draw the bars. The overflow indicators (...) are also drawn based on whether the start index is greater than 0 or the end index is less than the total number of data points.

From here, the event handler can also be simplified: 

Scroll Left: If horizontal_scroll_offset is less than data.len() - 1, increment it.

Scroll Right: If horizontal_scroll_offset is greater than 0, decrement it.

The event handler should not be performing width calculations, just modifying the scroll index.

The initial horizontal_scroll_offset for each ChartData struct should simply be set to data.len() - 1, ensuring the view starts at the far right of the data.

