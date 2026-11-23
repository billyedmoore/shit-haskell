{-# LANGUAGE TypeApplications #-}
answer :: Int
answer = 42
main :: IO ()
main = print (answer @Bool)
