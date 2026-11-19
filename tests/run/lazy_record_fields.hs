-- Laziness marks on record fields (time-manager's `handleKeyRef :: ~(IORef ..)` under StrictData).
{-# LANGUAGE StrictData, GADTs #-}
data Handle = Handle
  { timeout :: Int
  , action :: ~(Maybe String)
  , counts :: ~[Int]
  }

data G where
  G :: { gName :: ~String, gSize :: !Int } -> G

main :: IO ()
main = do
  let h = Handle 3 (Just "act") [1, 2]
  print (timeout h, action h, counts h)
  let lazyOne = Handle 4 undefined undefined
  print (timeout lazyOne)
  print (gName (G "g" 2), gSize (G "g" 2))
