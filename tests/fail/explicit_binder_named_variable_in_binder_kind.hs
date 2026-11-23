import Data.Proxy

f :: forall (a :: _k). Proxy a -> ()
f _ = ()

main :: IO ()
main = pure ()
