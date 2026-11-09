import System.Exit
import System.IO

main :: IO ()
main = do
  putStrLn "exiting with 3"
  hFlush stdout
  exitWith (ExitFailure 3)
