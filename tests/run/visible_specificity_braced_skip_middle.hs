{-# LANGUAGE TypeApplications, ExplicitForAll #-}
f :: forall a {b} c. a -> b -> c -> (a, b, c)
f a b c = (a, b, c)
value = f @Int @Bool 3 'x' True

main :: IO ()
main = print value
