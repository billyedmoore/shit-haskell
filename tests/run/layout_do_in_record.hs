-- Explicit `}` closes an aligned inner implicit `do` block inside record syntax.

data Rec = Rec { runRec :: Int -> IO Int }

mkRec :: Rec
mkRec = Rec
    { runRec = \x -> do
        let y = x + 1
        pure (y * 2)
    }

main :: IO ()
main = do
  n <- runRec mkRec 20
  print n
