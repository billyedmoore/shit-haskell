{-# LANGUAGE TypeApplications, ExplicitForAll #-}
f :: forall {a}. a -> a
f a = a
value = f @Bool True

main :: IO ()
main = print value
