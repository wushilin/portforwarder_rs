import { Component, OnInit, ɵɵsetComponentScope } from '@angular/core';
import { PFService, withLoading, trap, Listener, Listeners, DNS, reportError, reportSuccess, ListenerOk, SimpleResult, ListenerStatuses } from '../pf.service';
import { MatSnackBar } from '@angular/material/snack-bar';
import { MatDialog } from '@angular/material/dialog';
import { MatCard } from '@angular/material/card';
import { MatCardContent } from '@angular/material/card';
import { MatCardHeader } from '@angular/material/card';
import { MatCardFooter } from '@angular/material/card';
import { MatChipEditedEvent, MatChipInputEvent } from '@angular/material/chips';
import { COMMA, ENTER } from '@angular/cdk/keycodes';
import { forkJoin } from 'rxjs';
import { ConfirmationDialogComponent } from '../confirmdialog/confirmdialog.component';
@Component({
  selector: 'app-config',
  templateUrl: './configuration.component.html',
  styleUrls: ['./configuration.component.scss'],
})
export class ConfigurationComponent implements OnInit {
  newFrom: string = "";
  newTo: string = "";

  newListenerName: string = "";
  newListenerBind: string = "";
  newMaxIdleTimeMs: number = 3600000;
  newTargetPort: number = 443;


  separatorKeysCodes = [ENTER, COMMA] as const;
  targetAddOnBlur = true;
  displayedColumns: string[] = ['name', 'bind', 'targetPort', 'policy', 'maxIdleTime', 'rulesHosts', 'rulesPatterns'];
  dnsDisplayedColumns: string[] = ['from', 'to', 'action'];
  dns: [string, string][] = [];
  listenersList: [string, Listener][] = []
  ngOnInit(): void {
    console.log("Configuration Component Init is called");
    this.newListenerName = ""
    this.newListenerBind = ""
    this.newTargetPort = 443
    this.newMaxIdleTimeMs = 3600000
    this.fetchData()
  }

  fetchData() {
    forkJoin({
      dns:
        withLoading(
          () => this.pfService.getDNS()
        ),
      listeners:
        withLoading(
          () => this.pfService.getListeners()
        ),
    }).subscribe(results => {
      console.log("Fork Join completed")
      if (!results.dns) {
        reportError(this._snackBar, "Error fetching data", "Dismiss");
      } else {
        this.setDNS(results.dns);
      }

      if (!results.listeners) {
        reportError(this._snackBar, "Error fetching data", "Dismiss");
      } else {
        this.setListener(results.listeners);
      }
    });
  }

  addDNS() {
    console.log(`Add dns ${this.newFrom} => ${this.newTo}`)
    this.newFrom = this.newFrom.trim();
    this.newTo = this.newTo.trim();

    if (this.newFrom.trim() != "" && this.newTo.trim() != "") {
      this.replaceDns(this.newFrom, this.newTo);
    }
    this.newFrom = ""
    this.newTo = ""
  }

  saveData() {
    const dialogRef = this.dialog.open(ConfirmationDialogComponent, {
      data: {
        message: 'This will save the configuration to the server (write to file) but does not restart server.',
        buttonText: {
          ok: 'Save without restart',
          cancel: 'Don\'t save'
        }
      }
    });

    dialogRef.afterClosed().subscribe((confirmed: boolean) => {
      if (confirmed) {
        console.log("Restarting...");
        this.performSaveData();
      }
    });
  }

  performSaveData() {
    console.log(`I am saving data`);

    let dnsData: DNS = {};
    let listenerData: Listeners = {};
    this.dns.forEach((elem) => {
      let key = elem[0];
      let value = elem[1];
      console.log(`Adding dns ${key} - > ${value}`);
      dnsData[key] = value;
    })

    this.listenersList.forEach((elem) => {
      let name = elem[0];
      let listener = elem[1];
      console.log(`Adding listener ${name} -> ${listener}`);
      listenerData[name] = listener;
    });
    console.log(`final dns ${JSON.stringify(dnsData)} final listener ${JSON.stringify(listenerData)}`);

    console.log(`Saving DNS data...`)
    withLoading(
      () => this.pfService.saveDNS(dnsData)
    ).subscribe(result => {
      console.log("DNS done. Saving listener data...")
      withLoading(
        () => this.pfService.saveListeners(listenerData)
      ).subscribe(result => {
        console.log("Both done. Fetching new data...")
        reportSuccess(this._snackBar, "Configuration saved successfully", "dismiss");
        this.fetchData()
      })
    })
  }

  start() {
    const dialogRef = this.dialog.open(ConfirmationDialogComponent, {
      data: {
        message: 'If server is not started, it would be started now. No effect if it is already running',
        buttonText: {
          ok: 'Start',
          cancel: 'Don\'t start'
        }
      }
    });
    dialogRef.afterClosed().subscribe((confirmed: boolean) => {
      if (confirmed) {
        console.log("Starting...");
        withLoading(() =>
          this.pfService.start()
        ).subscribe({
          next: result => {
            console.log(`Raw start result is ${JSON.stringify(result)}`);
            const simpleResult = result as SimpleResult;
            if (simpleResult.success != undefined && simpleResult.changed != undefined) {
              reportSuccess(this._snackBar, "Server is already running. You need to stop it first.", "Dismiss");
              return;
            }
            const result1 = result as ListenerStatuses;
            var failedCount = 0
            var successCount = 0
            for (const key in result1) {
              const value = result1[key]!
              const valueOk = value as ListenerOk
              if (valueOk.Ok == undefined) {
                failedCount++
              } else {
                successCount++
              }
            }
            if (failedCount == 0) {
              reportSuccess(this._snackBar, `Server started, ${successCount} listeners OK`, "Dismiss");
            } else {
              reportError(this._snackBar, `Server started. ${successCount} listeners OK, ${failedCount} listeners failed`, "Dismiss")
            }
            console.log(`Start result is ${JSON.stringify(result)}`)
          },
          error: error => {
            reportError(this._snackBar, `Server can't be started`, "Dismiss");
          },
          complete: () => {
            console.log("OK");
          }
        });
      }
    });
  }
  stop() {
    const dialogRef = this.dialog.open(ConfirmationDialogComponent, {
      data: {
        message: 'This will stop all listeners. All connections will be force closed immediately.',
        buttonText: {
          ok: 'Stop Anyway',
          cancel: 'Don\'t stop'
        }
      }
    });
    dialogRef.afterClosed().subscribe((confirmed: boolean) => {
      if (confirmed) {
        console.log("Stopping...");
        withLoading(() =>
          this.pfService.stop()
        ).subscribe(result => {
          if (result.success) {
            if (result.changed) {
              reportSuccess(this._snackBar, `Server stopped`, "Dismiss");
            } else {
              reportSuccess(this._snackBar, `Server is already stopped`, "Dismiss");
            }
          } else {
            reportError(this._snackBar, `Can't stop server: ${result.message}`, "Dismiss");
          }
          console.log(`Stop result is ${result}`)
        });
      }
    });
  }
  restart() {
    const dialogRef = this.dialog.open(ConfirmationDialogComponent, {
      data: {
        message: 'This will restart server accroding to the last "Saved" configuration. Are you sure?\nAll connections will be interrupted.',
        buttonText: {
          ok: 'Restart Anyway',
          cancel: 'Don\'t restart'
        }
      }
    });

    dialogRef.afterClosed().subscribe((confirmed: boolean) => {
      if (confirmed) {
        console.log("Restarting...");
        withLoading(() =>
          this.pfService.restart()
        ).subscribe({next: result => {
          var failedCount = 0
          var successCount = 0
          for (const key in result) {
            const value = result[key]!
            const valueOk = value as ListenerOk
            if (valueOk.Ok == undefined) {
              failedCount++
            } else {
              successCount++
            }
          }
          if (failedCount == 0) {
            reportSuccess(this._snackBar, `Server restarted, ${successCount} listeners OK`, "Dismiss");
          } else {
            reportError(this._snackBar, `Server restarted. ${successCount} listeners OK, ${failedCount} listeners failed`, "Dismiss")
          }
          console.log(`Restart result is ${JSON.stringify(result)}`)
        },
        error: err => {
          reportError(this._snackBar, `Server can't be restarted.`, "Dismiss")
        }
      });
      }
    });
  }

  restore() {
    const dialogRef = this.dialog.open(ConfirmationDialogComponent, {
      data: {
        message: 'This will restore the server\'s config to last applied config and discard any unapplied config. Are you sure?',
        buttonText: {
          ok: 'Restore anyway',
          cancel: 'Don\'t restore'
        }
      }
    });

    dialogRef.afterClosed().subscribe((confirmed: boolean) => {
      if (confirmed) {
        console.log("Restoring...");
        withLoading(() => this.pfService.restore()).subscribe(result => {
          console.log(`Restore result ${JSON.stringify(result)}`)
          reportSuccess(this._snackBar, "Configuration restored successfully", "dismiss");
          this.fetchData();
        })
      }
    });

  }
  deleteListener(name: string) {
    var newListners = this.listenersList.filter((it) => it[0] != name);
    this.listenersList = newListners;
  }

  addListener() {
    this.newListenerName = this.newListenerName.trim()
    this.newListenerBind = this.newListenerBind.trim()
    if (this.newListenerBind == "" || this.newListenerName == "") {
      return;
    }
    console.log(`Add listener ${this.newListenerName} => ${this.newListenerBind}`)
    var find = this.listenersList.filter((it) => it[0] == this.newListenerName)
    if (find.length == 0) {
      this.listenersList.push([this.newListenerName, { bind: this.newListenerBind, policy: "DENY", target_port: this.newTargetPort, max_idle_time_ms: this.newMaxIdleTimeMs, rules: { static_hosts: [], patterns: [] } }])
      var tmp = this.listenersList.filter((it) => true)
      tmp.sort((a, b) => {
        return a[0].localeCompare(b[0])
      })
      this.listenersList = tmp
      console.log(`${this.listenersList}`)
      this.newListenerBind = ""
      this.newListenerName = ""
      this.newTargetPort = 443
      this.newMaxIdleTimeMs = 3600000
      console.log("FOrm reset")
    } else {
      console.log("Name already exists")
    }
  }
  replaceDns(from: string, to: string) {
    from = from.trim()
    to = to.trim()

    var removed = this.dns.filter((it) => it[0] != from)
    if (to != "") {
      removed.push([from, to])
    }
    removed.sort((a, b) => {
      return a[0].localeCompare(b[0])
    })
    this.dns = removed;
  }
  setListener(data: Listeners) {
    console.log(`Setting listener ${data} ${JSON.stringify(data)}`)
    var listenersListLocal: [string, Listener][] = [];
    for (const key in data) {
      const value = data[key]!
      listenersListLocal.push([key, value])
    }
    listenersListLocal.sort((a, b) => {
      return a[0].localeCompare(b[0])
    })
    this.listenersList = listenersListLocal
  }

  setDNS(data: DNS) {
    console.log(`Setting DNS ${data} ${JSON.stringify(data)}`)
    var dnsListLocal: [string, string][] = [];
    for (const key in data) {
      const value = data[key]!
      dnsListLocal.push([key, value])
    }
    dnsListLocal.sort((a, b) => {
      return a[0].localeCompare(b[0])
    })

    this.dns = dnsListLocal
  }

  editPattern(name: string, target: string, event: MatChipEditedEvent) {
    const value = event.value.trim();
    console.log(`edit ${name} -> ${target} -> ${value}`);

    // Remove fruit if it no longer has a name
    if (!value) {
      this.removePattern(name, target);
      return;
    }

    this.replacePattern(name, target, value);
    // Edit existing fruit
  }

  addPatternEvent(name: string, event: MatChipInputEvent) {
    const value = event.value.trim();
    if (value != "") {
      this.addPattern(name, value);
    }
    event.chipInput!.clear();
  }

  removePattern(name: string, target: string) {
    this.replacePattern(name, target, "")
  }

  addPattern(name: string, target: string) {
    const index = this.targetIndex(name)
    if (index != -1) {
      const element = this.listenersList[index]
      const targets = element[1].rules.patterns;
      if (!targets.includes(target)) {
        targets.push(target);
      }
    }
  }
  replacePattern(name: string, target: string, new_value: string) {
    const index = this.targetIndex(name)
    if (index != -1) {
      const element = this.listenersList[index]
      const targets = element[1].rules.patterns;
      if (new_value == "") {
        element[1].rules.patterns = targets.filter((i) => i != target)
      } else {
        element[1].rules.patterns = targets.map((i) => {
          if (i == target) {
            return new_value;
          } else {
            return i;
          }
        })
      }
    }
  }
  editStaticHost(name: string, target: string, event: MatChipEditedEvent) {
    const value = event.value.trim();
    console.log(`edit ${name} -> ${target} -> ${value}`);

    // Remove fruit if it no longer has a name
    if (!value) {
      this.removeStaticHost(name, target);
      return;
    }

    this.replaceStaticHost(name, target, value);
    // Edit existing fruit
  }

  addStaticHostEvent(name: string, event: MatChipInputEvent) {
    const value = event.value.trim();
    if (value != "") {
      this.addStaticHost(name, value);
    }
    event.chipInput!.clear();
  }

  removeStaticHost(name: string, target: string) {
    this.replaceStaticHost(name, target, "")
  }

  addStaticHost(name: string, target: string) {
    const index = this.targetIndex(name)
    if (index != -1) {
      const element = this.listenersList[index]
      const targets = element[1].rules.static_hosts;
      if (!targets.includes(target)) {
        targets.push(target);
      }
    }
  }
  replaceStaticHost(name: string, target: string, new_value: string) {
    const index = this.targetIndex(name)
    if (index != -1) {
      const element = this.listenersList[index]
      const targets = element[1].rules.static_hosts;
      if (new_value == "") {
        element[1].rules.static_hosts = targets.filter((i) => i != target)
      } else {
        element[1].rules.static_hosts = targets.map((i) => {
          if (i == target) {
            return new_value;
          } else {
            return i;
          }
        })
      }
    }
  }

  targetIndex(name: string): number {
    var rindex = -1;
    this.listenersList.forEach((element, index) => {
      if (element[0] == name) {
        rindex = index;
      }
    })
    return rindex;
  }
  constructor(
    public pfService: PFService,
    private _snackBar: MatSnackBar,
    private dialog: MatDialog,
  ) { }
}
