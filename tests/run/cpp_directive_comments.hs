{-# LANGUAGE CPP #-}
-- C comments in directives are not part of them (GHC's MachRegs.h:
-- `#define MAX_REAL_VANILLA_REG 1 /* always, since it defines the entry conv */`).
module Main where

#define REGS 1 /* always, since it defines the entry conv */
#define NAME "regs" /* a string */

main :: IO ()
main = do
#if REGS < 2 /* few */
  putStrLn "few"
#else
  putStrLn "many"
#endif
  print (REGS + 1 :: Int, NAME)
