import {-# SOURCE #-} BootRenamedConstraints (render)

main :: IO ()
main = putStrLn (render (3 :: Int))
