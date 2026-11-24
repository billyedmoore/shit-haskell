{-# LANGUAGE TypeApplications, ExplicitForAll #-}
f :: forall {a} b. a -> b -> (a, b)
f a b = (a, b)
value = f @Bool 'x' True

main :: IO ()
main = print value
