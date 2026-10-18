package main

import (
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"reflect"
	"testing"

	tea "github.com/charmbracelet/bubbletea"
)

func TestParseTaskIDs(t *testing.T) {
	got, err := parseTaskIDs("2, 5,8")
	if err != nil {
		t.Fatal(err)
	}
	if want := []int{2, 5, 8}; !reflect.DeepEqual(got, want) {
		t.Fatalf("parseTaskIDs() = %v, want %v", got, want)
	}
}

func TestParseTaskIDsRejectsInvalidAndDuplicateIDs(t *testing.T) {
	for _, input := range []string{"", "1,wat", "-1", "4,4"} {
		if _, err := parseTaskIDs(input); err == nil {
			t.Errorf("parseTaskIDs(%q) succeeded, want error", input)
		}
	}
}

func TestMakeDispatchRequestIncludesSubjectiveTimeScale(t *testing.T) {
	request, err := makeDispatchRequest([3]string{"3,4", "Review this request", "0.5"})
	if err != nil {
		t.Fatal(err)
	}
	if !reflect.DeepEqual(request.TaskIDs, []int{3, 4}) || request.Context != "Review this request" || request.TimeDilationFactor != 0.5 {
		t.Fatalf("unexpected dispatch request: %+v", request)
	}
}

func TestRequestValidationRejectsInvalidTimeScale(t *testing.T) {
	for _, scale := range []string{"0", "100.1", "NaN", "invalid"} {
		if _, err := makeDispatchRequest([3]string{"1", "Do the requested work", scale}); err == nil {
			t.Errorf("time scale %q succeeded, want validation error", scale)
		}
	}
}

func TestTypingQDoesNotQuitTheRequestEditor(t *testing.T) {
	m := initialModel("")
	updated, command := m.Update(tea.KeyMsg{Type: tea.KeyRunes, Runes: []rune{'q'}})
	if command != nil {
		t.Fatal("typing should not produce a command")
	}
	if updated.(model).fields[0] != "q" {
		t.Fatalf("typed character was not added to the task field: %+v", updated)
	}
}

func TestDispatchCommandPostsRequestToRustAPI(t *testing.T) {
	var received dispatchRequest
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.Method != http.MethodPost || r.URL.Path != "/v1/dispatch" {
			t.Errorf("unexpected request: %s %s", r.Method, r.URL.Path)
		}
		if err := json.NewDecoder(r.Body).Decode(&received); err != nil {
			t.Errorf("decode request: %v", err)
		}
		w.Header().Set("Content-Type", "application/json")
		_, _ = w.Write([]byte(`[{"task_id":3,"success":true,"timing":{"dilation_factor":2.0}}]`))
	}))
	defer server.Close()

	m := initialModel(server.URL)
	m.fields = [3]string{"3", "Act on the submitted request", "2.0"}
	m, command := m.startDispatch()
	if !m.busy {
		t.Fatal("model should enter busy state during dispatch")
	}
	message := command().(dispatchResultMsg)
	if message.err != nil {
		t.Fatal(message.err)
	}
	if received.TimeDilationFactor != 2 || received.Context != "Act on the submitted request" {
		t.Fatalf("API received unexpected request: %+v", received)
	}
}
