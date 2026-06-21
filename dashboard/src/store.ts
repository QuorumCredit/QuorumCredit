import { configureStore } from "@reduxjs/toolkit";
import loanReducer from "./loanSlice";

export const store = configureStore({
  reducer: { loans: loanReducer },
});

export type RootState = ReturnType<typeof store.getState>;
export type AppDispatch = typeof store.dispatch;
