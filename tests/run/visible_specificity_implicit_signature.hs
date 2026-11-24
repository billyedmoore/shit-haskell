{-# LANGUAGE TypeApplications, ExplicitForAll #-}
f :: a -> a
f x = x
value = f @Int 3

main :: IO ()
main = print value
