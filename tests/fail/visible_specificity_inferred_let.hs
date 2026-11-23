{-# LANGUAGE TypeApplications, ExplicitForAll #-}
value = let f x = x in f @Int 3

main :: IO ()
main = print value
