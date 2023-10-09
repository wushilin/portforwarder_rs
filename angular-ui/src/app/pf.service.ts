import { Injectable } from '@angular/core';
import { Observable, of } from 'rxjs';
import { HttpClient, HttpHeaders } from '@angular/common/http';
import { catchError, map, tap, finalize } from 'rxjs/operators';
import {MatSnackBar} from '@angular/material/snack-bar';

export function reportError<T>(theBar:MatSnackBar, message: string, action:string):Observable<T> {
    console.log("error => " + message);
    theBar.open(message, action, {duration: 300000, panelClass: "error-snack-bar"});
    return of({} as T);
  }

export function withLoading<T>(functionObject:TrapFunc<T>, errorer?:ErrorFunc<T>):Observable<T> {
    return trap(functionObject, showLoading, hideLoading , errorer)
}

export function trap<T>(functionObject: TrapFunc<T>, starter?:()=>void, ender?:()=>void, errorer?:ErrorFunc<T>):Observable<T> {
    if(starter) {
       starter();
    }
    if(errorer) {
      console.log("With errorer " + errorer);
      return functionObject().pipe(
        tap(result => console.log(`Trap Begin...`)),
        catchError(errorer),
        finalize(() => {
          console.log(`Trap End..`);
          if(ender) {
            ender();
          }
        })
      );
    } else {
      return functionObject().pipe(
        tap(result => console.log(`Trap Begin...`)),
        finalize(() => {
          console.log(`Trap End..`);
          if(ender) {
            ender();
          }
        })
      );
    }
  }
export function showLoading() {
    console.log("Showing loading...")
    document.getElementById("loading-layer")!.style!.display = "block";
  }
export function hideLoading() {
    console.log("Hiding loading...")
    document.getElementById("loading-layer")!.style!.display = "none";
}

export function reportSuccess(theBar:MatSnackBar, message:string, action: string) {
    console.log("success => " + message);
    theBar.open(message, action, {duration: 300000, panelClass: "error-snack-bar"});
  }

type TrapFunc<T> = () => Observable<T>;
type ErrorFunc<T> = (error: any) => Observable<T>;

export interface Listener {
  bind: string,
  targets: string[]
}
//{"test1":{"Ok":true},"awefawef":{"Err":{"message":"invalid socket address"}},"l1":{"Ok":true}}
export interface ListenerErrorMessage {
  message:string
}

export interface ListenerError {
  Err: ListenerErrorMessage
}
export interface ListenerOk {
  Ok: boolean
}

export type ListenerStatus = ListenerOk | ListenerError;

export type ListenerStatuses = Record<string, ListenerStatus>;


export interface Stats {
  name: string,
  total: number,
  active: number,
  downloaded_bytes: number,
  uploaded_bytes: number,
}


export type StatsResponse = Record<string, Stats>;
export type Listeners = Record<string, Listener>;

export type DNS = Record<string, string>;
export type StartResult = SimpleResult | ListenerStatuses;

export interface SimpleResult {
  success: boolean,
  changed: boolean,
  message: string|undefined,
}

@Injectable({ providedIn: 'root' })
export class PFService {
  private baseUrl = '/apiserver';  // URL to web api
  httpOptions = {
    headers: new HttpHeaders({ 'Content-Type': 'application/json' })
  };

  constructor(private http: HttpClient) { }

  getListeners(): Observable<Listeners> {
    return this.http.get<Listeners>(this.baseUrl + "/config/listeners",)
          .pipe(
            tap(result => this.log(`fetched listeners ${JSON.stringify(result)}`)),
    );
  }

  getStats():Observable<StatsResponse> {
    return this.http.get<StatsResponse>(this.baseUrl + "/stats/listeners")
      .pipe(tap(result => this.log(`fetched stats ${JSON.stringify(result)}`)),
      );
  }

  getDNS():Observable<DNS> {
    // DELETE /rest/youtubeChannel/byId/18
    return this.http.get<DNS>(this.baseUrl + "/config/dns")
      .pipe(tap(result => this.log(`fetched dns ${JSON.stringify(result)}`)),
      );
  }
  
  saveDNS(target:DNS):Observable<DNS> {
    var body = target;
    var bodyJson = JSON.stringify(body);
    console.log(`After conversion: ${JSON.stringify(bodyJson)}`);
    return this.http.put<DNS>(this.baseUrl + "/config/dns", bodyJson)
      .pipe(tap(result => this.log(`Update DNS:${target} result ${JSON.stringify(result)}`)),
      );
  }

  getListenerStatuses():Observable<ListenerStatuses> {
    return this.http.get<ListenerStatuses>(this.baseUrl + "/status/listeners")
      .pipe(tap(result => this.log(`Get listener statuses ${JSON.stringify(result)}`)),
      );
  }
  saveListeners(target:Listeners):Observable<Listeners> {
    var body = target;
    var bodyJson = JSON.stringify(body);
    console.log(`After conversion: ${JSON.stringify(bodyJson)}`);
    return this.http.put<Listeners>(this.baseUrl + "/config/listeners", bodyJson)
      .pipe(tap(result => this.log(`Update Listeners:${target} result ${JSON.stringify(result)}`)),
      );
  }

  restore():Observable<string> {
    return this.http.post<string>(this.baseUrl + "/config/reset", "")
      .pipe(tap(result => this.log(`Reset config result ${result}`)),
    );
  }

  restart():Observable<ListenerStatuses> {
    return this.http.post<ListenerStatuses>(this.baseUrl + "/config/apply", "")
      .pipe(tap(result => this.log(`Restart result ${result}`)),
    );
  }
  stop():Observable<SimpleResult> {
    return this.http.post<SimpleResult>(this.baseUrl + "/config/stop", "")
      .pipe(tap(result => this.log(`Stop result ${result}`)),
    );
  }

  start():Observable<StartResult> {
    return this.http.post<StartResult>(this.baseUrl + "/config/start", "")
      .pipe(tap(result => this.log(`Start result ${JSON.stringify(result)}`)),
    );
  }
  private log(message: string) {
    console.log(`PFService: ${message}`);
  }

  private handleError<T>(operation = 'operation', result?: T) {
    return (error: any): Observable<T> => {
      console.error(error); // log to console instead
      this.log(`${operation} failed: ${error.message}`);
      return of(result as T);
    };
  }
}
