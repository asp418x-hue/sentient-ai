package main

import (
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"math"
	"net/http"
	"os"
	"strconv"
	"strings"
	"time"

	tea "github.com/charmbracelet/bubbletea"
	"github.com/charmbracelet/lipgloss"
)

const defaultAPIURL = "http://127.0.0.1:8080"

type dispatchRequest struct {
	TaskIDs            []int   `json:"task_ids"`
	Context            string  `json:"context"`
	MaxConcurrent      int     `json:"max_concurrent"`
	TimeDilationFactor float64 `json:"time_dilation_factor"`
}

type dispatchResultMsg struct {
	body []byte
	err  error
}

type model struct {
	fields  [3]string
	focus   int
	busy    bool
	message string
	isError bool
	apiURL  string
}

func initialModel(apiURL string) model {
	if apiURL == "" {
		apiURL = defaultAPIURL
	}
	return model{fields: [3]string{"", "", "1.0"}, apiURL: strings.TrimRight(apiURL, "/")}
}

func (m model) Init() tea.Cmd { return nil }

func (m model) Update(msg tea.Msg) (tea.Model, tea.Cmd) {
	switch msg := msg.(type) {
	case dispatchResultMsg:
		m.busy = false
		m.isError = msg.err != nil
		if msg.err != nil {
			m.message = msg.err.Error()
		} else {
			var pretty bytes.Buffer
			if err := json.Indent(&pretty, msg.body, "", "  "); err != nil {
				m.message = string(msg.body)
			} else {
				m.message = pretty.String()
			}
		}
		return m, nil
	case tea.KeyMsg:
		if m.busy {
			if msg.String() == "ctrl+c" || msg.String() == "esc" {
				return m, tea.Quit
			}
			return m, nil
		}
		switch msg.String() {
		case "ctrl+c", "esc":
			return m, tea.Quit
		case "tab", "shift+tab":
			if msg.String() == "shift+tab" {
				m.focus = (m.focus + len(m.fields) - 1) % len(m.fields)
			} else {
				m.focus = (m.focus + 1) % len(m.fields)
			}
			return m, nil
		case "ctrl+d":
			return m.startDispatch()
		case "enter":
			if m.focus == 0 {
				m.focus = 1
				return m, nil
			}
			if m.focus == 1 {
				m.fields[1] += "\n"
				return m, nil
			}
			return m.startDispatch()
		case "backspace", "delete":
			m.fields[m.focus] = dropLastRune(m.fields[m.focus])
			return m, nil
		}
		if msg.Type == tea.KeyRunes {
			m.fields[m.focus] += string(msg.Runes)
			return m, nil
		}
	}
	return m, nil
}

func (m model) View() string {
	title := lipgloss.NewStyle().Bold(true).Foreground(lipgloss.Color("6")).Render("Sentient-AI · Request dispatch")
	labels := []string{"Task IDs (comma separated)", "Request", "Macro time scale (e.g. 0.5 contracts, 2 expands)"}
	var sections []string
	for i, label := range labels {
		style := lipgloss.NewStyle().Border(lipgloss.RoundedBorder()).Padding(0, 1).Width(68)
		if i == m.focus {
			style = style.BorderForeground(lipgloss.Color("6"))
		} else {
			style = style.BorderForeground(lipgloss.Color("8"))
		}
		value := m.fields[i]
		if value == "" {
			value = " "
		}
		sections = append(sections, lipgloss.NewStyle().Bold(true).Render(label)+"\n"+style.Render(value))
	}
	help := "Tab: next field • Enter: advance/dispatch • Ctrl+D: dispatch • Esc: quit"
	if m.busy {
		help = "Dispatching agents… • Ctrl+C: quit"
	}
	output := ""
	if m.message != "" {
		style := lipgloss.NewStyle().Border(lipgloss.NormalBorder()).Padding(0, 1).Width(68)
		if m.isError {
			style = style.BorderForeground(lipgloss.Color("1"))
		} else {
			style = style.BorderForeground(lipgloss.Color("2"))
		}
		output = "\n" + style.Render(m.message)
	}
	return lipgloss.NewStyle().Padding(1, 2).Render(strings.Join(append([]string{title}, sections...), "\n\n") + "\n\n" + lipgloss.NewStyle().Faint(true).Render(help) + output)
}

func (m model) startDispatch() (model, tea.Cmd) {
	request, err := makeDispatchRequest(m.fields)
	if err != nil {
		m.message = err.Error()
		m.isError = true
		return m, nil
	}
	m.busy = true
	m.message = ""
	m.isError = false
	apiURL := m.apiURL
	return m, func() tea.Msg {
		ctx, cancel := context.WithTimeout(context.Background(), 10*time.Minute)
		defer cancel()
		body, err := json.Marshal(request)
		if err != nil {
			return dispatchResultMsg{err: err}
		}
		req, err := http.NewRequestWithContext(ctx, http.MethodPost, apiURL+"/v1/dispatch", bytes.NewReader(body))
		if err != nil {
			return dispatchResultMsg{err: err}
		}
		req.Header.Set("Content-Type", "application/json")
		response, err := http.DefaultClient.Do(req)
		if err != nil {
			return dispatchResultMsg{err: fmt.Errorf("dispatch request failed: %w", err)}
		}
		defer response.Body.Close()
		var responseBody bytes.Buffer
		if _, err := responseBody.ReadFrom(response.Body); err != nil {
			return dispatchResultMsg{err: fmt.Errorf("could not read dispatch response: %w", err)}
		}
		if response.StatusCode < 200 || response.StatusCode >= 300 {
			return dispatchResultMsg{err: fmt.Errorf("dispatch API returned %s: %s", response.Status, strings.TrimSpace(responseBody.String()))}
		}
		return dispatchResultMsg{body: responseBody.Bytes()}
	}
}

func makeDispatchRequest(fields [3]string) (dispatchRequest, error) {
	taskIDs, err := parseTaskIDs(fields[0])
	if err != nil {
		return dispatchRequest{}, err
	}
	contextText := strings.TrimSpace(fields[1])
	if contextText == "" {
		return dispatchRequest{}, fmt.Errorf("request text cannot be empty")
	}
	scale, err := strconv.ParseFloat(strings.TrimSpace(fields[2]), 64)
	if err != nil || math.IsNaN(scale) || math.IsInf(scale, 0) || scale < 0.01 || scale > 100 {
		return dispatchRequest{}, fmt.Errorf("macro time scale must be a number between 0.01 and 100")
	}
	return dispatchRequest{
		TaskIDs: taskIDs, Context: contextText, MaxConcurrent: 8, TimeDilationFactor: scale,
	}, nil
}

func parseTaskIDs(input string) ([]int, error) {
	parts := strings.Split(input, ",")
	ids := make([]int, 0, len(parts))
	seen := make(map[int]struct{}, len(parts))
	for _, part := range parts {
		id, err := strconv.Atoi(strings.TrimSpace(part))
		if err != nil || id < 0 {
			return nil, fmt.Errorf("task IDs must be comma-separated nonnegative integers")
		}
		if _, exists := seen[id]; exists {
			return nil, fmt.Errorf("task IDs must be unique")
		}
		seen[id] = struct{}{}
		ids = append(ids, id)
	}
	if len(ids) == 0 || len(ids) > 256 {
		return nil, fmt.Errorf("enter between 1 and 256 task IDs")
	}
	return ids, nil
}

func dropLastRune(s string) string {
	if s == "" {
		return s
	}
	_, size := lastRune(s)
	return s[:len(s)-size]
}

func lastRune(s string) (rune, int) {
	for i := len(s) - 1; i >= 0; i-- {
		if s[i]&0xC0 != 0x80 {
			return []rune(s[i:])[0], len(s) - i
		}
	}
	return rune(s[0]), 1
}

func main() {
	apiURL := os.Getenv("SENTIENT_AI_API")
	program := tea.NewProgram(initialModel(apiURL), tea.WithAltScreen())
	if _, err := program.Run(); err != nil {
		fmt.Fprintln(os.Stderr, "TUI failed:", err)
		os.Exit(1)
	}
}
