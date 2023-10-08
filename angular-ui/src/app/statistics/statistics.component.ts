import { Component, OnInit } from '@angular/core';
import { PFService, withLoading, Stats, StatsResponse, ListenerStatuses, Listeners, ListenerStatus, ListenerOk, ListenerError, trap, Listener, reportError, reportSuccess } from '../pf.service';
import { MatSnackBar } from '@angular/material/snack-bar';
import { MatDialog } from '@angular/material/dialog';
import { MatCard } from '@angular/material/card';
import { MatCardContent } from '@angular/material/card';
import { MatCardHeader } from '@angular/material/card';
import { MatCardFooter } from '@angular/material/card';
import { forkJoin } from 'rxjs';
@Component({
  selector: 'app-stats',
  templateUrl: './statistics.component.html',
  styleUrls: ['./statistics.component.scss'],
})
export class StatisticsComponent implements OnInit {
  displayedColumns: string[] = ['name', 'total', 'active', 'downloaded', 'uploaded']
  failedDisplayedColumns: string[] = ['name', 'reason']
  listeners:[string, Listener][] = []
  updateTimestamp: string = ""
  counter: number = 0
  failedListeners:[string, string][] = []
  stats:Stats[] = []
  ngOnInit(): void {
    console.log("Configuration Component Init is called");
    this.refresh()
  }

  setStats(resp:StatsResponse) {
    var localResult:Stats[] = []
    for(const key in resp) {
      const value = resp[key]!
      localResult.push(value)
    }
    localResult.sort ((a, b) => {
      return a.name.localeCompare(b.name);
    });
    this.stats = localResult;
  }
  setFailed(status:ListenerStatuses) {
    var localResult:[string, string][] = []
    for(const key in status) {
      const value = status[key]!
      const valueAsOk = value as ListenerOk
      if(valueAsOk.Ok !== undefined) {
        continue;
      }
      const valueAsErr = value as ListenerError;
      var message = valueAsErr.Err.message
      localResult.push([key, message])
    }
    console.log(`Updating failed to ${JSON.stringify(localResult)}`);
    this.failedListeners = localResult;
  }
  refresh() {
    setTimeout(() => this.refresh(), 3000)
    this.pfService.getStats().subscribe(result => {
      this.setStats(result)
      var newDate = new Date();
      this.updateTimestamp = newDate.toUTCString();
    });
    this.pfService.getListenerStatuses().subscribe(result => {
      this.setFailed(result);
    });
    this.pfService.getListeners().subscribe(result => {
      this.setListeners(result)
    })
    console.log(`Refreshing...`)
    this.counter = this.counter+ 1
  }

  setListeners(list:Listeners) {
    var local:[string, Listener][] = []
    for(const key in list) {
      const value = list[key]!
      local.push([key, value])
    }
    this.listeners = local
    console.log(`Listener data ${JSON.stringify(this.listeners)}`)
  }

  getBind(name:string):string {
    let find = this.listeners.filter((entry) => {
      return entry[0] == name
    })
    if(find.length == 0) {
      return ""
    }
    return find[0][1].bind
  }
  constructor(
    public pfService: PFService,
    private _snackBar: MatSnackBar,
    private dialog: MatDialog,
  ) { }
}
