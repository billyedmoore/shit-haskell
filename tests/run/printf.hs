import Text.Printf

main :: IO ()
main = do
  printf "%d items cost %.2f\n" (3 :: Int) (9.5 :: Double)
  printf "%5s|%-5s|\n" "ab" "cd"
  printf "%x %o %c %%\n" (255 :: Int) (8 :: Int) 'z'
  let s = printf "%03d" (7 :: Int) :: String
  putStrLn s
