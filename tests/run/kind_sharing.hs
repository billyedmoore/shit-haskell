module Main where
import SharedKinds

main :: IO ()
main = do
  print (unwrap (Higher (Just (7 :: Int))))
  print (unwrap (Higher [True, False]))
  print (getCompose (Compose (Just [1, 2 :: Int])))
  print (getCompose (Compose [Just True, Nothing]))
  print (edges first, edges second)
