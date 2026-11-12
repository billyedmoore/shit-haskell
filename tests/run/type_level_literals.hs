{-# LANGUAGE DataKinds, TypeOperators, TypeApplications, ScopedTypeVariables, NoStarIsType #-}
import GHC.TypeLits
import Data.Proxy
import Data.Type.Ord ()
main = do
  print (natVal (Proxy :: Proxy (1 + 1)), natVal (Proxy :: Proxy (3 * 4 - 2)), natVal (Proxy :: Proxy (2 ^ 10)))
  print (natVal (Proxy :: Proxy (Div 17 5)), natVal (Proxy :: Proxy (Mod 17 5)), natVal (Proxy :: Proxy (Log2 1024)))
  print (symbolVal (Proxy :: Proxy (AppendSymbol "foo" "bar")), symbolVal (Proxy :: Proxy (ConsSymbol 'x' "yz")))
  print (natVal (Proxy :: Proxy (CharToNat 'A')), charVal (Proxy :: Proxy (NatToChar 66)))
