import Control.Concurrent
import Control.Monad
import System.Exit
import System.IO
import System.Posix.Signals
import System.Posix.Process

main :: IO ()
main = do
  -- A handler runs in a thread of its own when the signal arrives.
  m <- newEmptyMVar
  _ <- installHandler sigUSR1 (Catch (putMVar m "usr1")) Nothing
  raiseSignal sigUSR1
  takeMVar m >>= putStrLn
  -- CatchInfo sees the signal number.
  _ <- installHandler sigUSR2 (CatchInfo (\i -> putMVar m (show (siginfoSignal i == sigUSR2)))) Nothing
  raiseSignal sigUSR2
  takeMVar m >>= putStrLn
  -- Blocked signals stay pending until unblocked.
  blockSignals (addSignal sigUSR1 emptySignalSet)
  raiseSignal sigUSR1
  pend <- getPendingSignals
  print (inSignalSet sigUSR1 pend)
  unblockSignals (addSignal sigUSR1 emptySignalSet)
  takeMVar m >>= putStrLn
  -- The previous handler is answered when a new one is installed.
  old <- installHandler sigUSR1 Default Nothing
  putStrLn (case old of Catch _ -> "was Catch"; _ -> "was other")
  -- forkProcess: the child's exit code reaches the parent.
  hFlush stdout
  p <- forkProcess (putStrLn "in child" >> exitWith (ExitFailure 3))
  st <- getProcessStatus True False p
  print st
  q <- forkProcess (return ())
  getProcessStatus True False q >>= print
