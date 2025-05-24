import matplotlib.pyplot as plt
import numpy as np

def plot(data,vertical_offset=0.1):
    # Create a figure and axis
    fig, ax = plt.subplots()

    # Plot each line with an offset and markers
    for i, line_data in enumerate(data):
        y_values = np.array(line_data) + i * vertical_offset
        ax.plot(y_values, marker='o', label=f'Line {i+1}')  # 'o' marks data points

    # Add labels and legend
    ax.set_xlabel('Index')
    ax.set_ylabel('Value (with offset)')
    ax.set_title('Samples')

    plt.grid(True, linestyle='--', alpha=0.6)
    plt.tight_layout()
    plt.show()

def plot2(data,vertical_offset=0.1):
    fig, ax = plt.subplots()
    lines = []  # Store line objects for later reference

    # Plot each line with markers and store line objects
    for i, line_data in enumerate(data):
        y_values = np.array(line_data, dtype=float) + i * vertical_offset
        line, = ax.plot(y_values, marker='o', label=f'{i+1}', picker=5)  # `picker=5` makes points clickable
        lines.append(line)

    # Event handler for clicks
    def on_click(event):
        if event.inaxes != ax:
            return
        for line in lines:
            if line.contains(event)[0]:  # Check if click is on this line
                x_data, y_data = line.get_data()
                x_click = int(round(event.xdata))  # Nearest data point index
                if 0 <= x_click < len(x_data):  # Ensure index is valid
                    y_click = y_data[x_click] - (list(lines).index(line) * vertical_offset)  # Remove offset
                    print(f"Clicked Line {list(lines).index(line)+1}, Point {x_click}: Value = {y_click:.2f}")

    # Connect the event handler
    fig.canvas.mpl_connect('button_press_event', on_click)

    ax.set_xlabel('Index')
    ax.set_ylabel('Value (with offset)')
    plt.grid(True, linestyle='--', alpha=0.6)
    plt.tight_layout()
    plt.show()

if __name__ == '__main__':
    import csv,fileinput
    r = csv.reader(fileinput.input())
    rows  = [ [ float(v) for v in row ] for row in r]
    plot(rows)
   
